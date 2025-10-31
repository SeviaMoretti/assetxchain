//! # Data Assets Pallet
//!
//! A pallet for managing data assets with an independent asset state tree.
//! 
//! ## Overview
//! 
//! This pallet implements a dual-layer MPT structure:
//! - Main Asset Trie: Stores all data assets
//! - Certificate Sub-Tries: Each asset has its own sub-trie for certificates
//! 
//! All data is stored in Child Tries, completely independent from the main state_root.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;
use alloc::vec::Vec;

pub use pallet::*;
pub mod types;
pub mod digest_item;
pub mod collateral;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub use collateral::BalanceOf;

// 需要和 runtime/src/lib.rs 中的对应值保持一致
pub const MILLI_SECS_PER_BLOCK: u64 = 18000;

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::pallet_prelude::*;
    use frame_system::pallet_prelude::*;
    use sp_core::H256;
    use frame_support::storage::child;
    use sp_runtime::traits::{SaturatedConversion, Saturating};
    use frame_support::traits::{Currency, ReservableCurrency};
    
    use crate::types::*;

    const ASSET_TRIE_ID: &[u8] = b":asset_trie:";
    const CERTIFICATE_TRIE_PREFIX: &[u8] = b":certificate_trie:";
    const METADATA_PREFIX: &[u8] = b"_metadata/";

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::config]
    pub trait Config: frame_system::Config + pallet_timestamp::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
        
        /// Currency type for handling collateral
        type Currency: Currency<Self::AccountId> + ReservableCurrency<Self::AccountId>;
        
        /// Base collateral amount (e.g., 2000 DATA)
        #[pallet::constant]
        type BaseCollateral: Get<BalanceOf<Self>>;
        
        /// Collateral per MB of data (e.g., 100 DATA/MB)
        #[pallet::constant]
        type CollateralPerMB: Get<BalanceOf<Self>>;
        
        /// Maximum collateral cap (e.g., 75000 DATA)
        #[pallet::constant]
        type MaxCollateral: Get<BalanceOf<Self>>;

        #[pallet::constant]
        type MaxNameLength: Get<u32>;
        
        #[pallet::constant]
        type MaxDescriptionLength: Get<u32>;
    }

    /// Storage for asset collateral information
    #[pallet::storage]
    #[pallet::getter(fn asset_collateral)]
    pub type AssetCollateral<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        [u8; 32], // asset_id
        CollateralInfo<T::AccountId, BalanceOf<T>, BlockNumberFor<T>>,
    >;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        AssetRegistered { asset_id: [u8; 32], token_id: u32, owner: T::AccountId, collateral: BalanceOf<T> },
        CertificateIssued { asset_id: [u8; 32], certificate_id: [u8; 32], issuer: T::AccountId, holder: T::AccountId },
        AssetTransferred { asset_id: [u8; 32], from: T::AccountId, to: T::AccountId },
        CertificateRevoked { asset_id: [u8; 32], certificate_id: [u8; 32] },
        AssetRootUpdated { root: H256 },
        /// Collateral locked for asset
        CollateralLocked { asset_id: [u8; 32], depositor: T::AccountId, amount: BalanceOf<T> },
        /// Collateral released (phase completed)
        CollateralReleased { asset_id: [u8; 32], amount: BalanceOf<T>, phase: u8 },
        /// Collateral slashed due to violation
        CollateralSlashed { asset_id: [u8; 32], amount: BalanceOf<T>, percentage: u8 },
        CollateralOverCappedHint {
            asset_id: [u8; 32], depositor: T::AccountId, total_uncapped: BalanceOf<T>, capped_amount: BalanceOf<T>, max_collateral: BalanceOf<T> },
    }

    #[pallet::error]
    pub enum Error<T> {
        AssetNotFound,
        AssetNotActive,
        AssetLocked,
        CertificateNotFound,
        NotOwner,
        InvalidInput,
        NameTooLong,
        DescriptionTooLong,
        InvalidRightType,

        InsufficientBalance,
        CollateralNotFound,
        InvalidSlashPercentage,
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn on_initialize(n: BlockNumberFor<T>) -> Weight {
            // Process collateral releases
            let release_weight = Self::process_collateral_releases(n);
            
            release_weight
        }
        
        fn on_finalize(_n: BlockNumberFor<T>) {
            //计算asset root
            let root = Self::compute_asset_root();
            
            //创建digest item并添加到区块头的digest中
            let digest_item = crate::digest_item::create_asset_root_digest(root);
            frame_system::Pallet::<T>::deposit_log(digest_item);
            
            //事件
            Self::deposit_event(Event::AssetRootUpdated { root });
        }
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(10_000)]
        pub fn register_asset(
            origin: OriginFor<T>,
            name: Vec<u8>,
            description: Vec<u8>,
            raw_data_hash: H256,
            data_size_bytes: u64, // 应该该有cid、encryptioninfo等信息
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            
            ensure!(
                name.len() <= T::MaxNameLength::get() as usize,
                Error::<T>::NameTooLong
            );
            ensure!(
                description.len() <= T::MaxDescriptionLength::get() as usize,
                Error::<T>::DescriptionTooLong
            );
            
            let timestamp = Self::current_timestamp();
            let asset_id = DataAsset::generate_asset_id(&who, timestamp, &raw_data_hash);
            // Check if asset already exists
            ensure!(Self::get_asset(&asset_id).is_none(), Error::<T>::InvalidInput);
            // Get collateral amount for event
            let (collateral_amount, is_over_capped) = Self::calculate_collateral(data_size_bytes);
            if is_over_capped {
                // 获取上限值，用于事件中展示“原计算值vs上限值”
                let max_collateral = T::MaxCollateral::get();
                // 重新计算“未封顶的原始金额”（用于提示用户“原本需要多少，实际锁定多少”）
                let data_size_mb = ((data_size_bytes as u128) / (1024 * 1024)).max(1);
                let variable_collateral = T::CollateralPerMB::get()
                    .saturating_mul(data_size_mb.saturated_into());
                let total_uncapped = T::BaseCollateral::get()
                    .saturating_add(variable_collateral);
                
                // 发射超限提示事件（需在 Event 枚举中新增该事件）
                Self::deposit_event(Event::CollateralOverCappedHint {
                    asset_id,
                    depositor: who.clone(),
                    total_uncapped,    // 未封顶的原始计算值（如102000DAT）
                    capped_amount: collateral_amount, // 封顶后的实际锁定值（如50000DAT）
                    max_collateral,    // 质押金上限（如50000DAT）
                });
            }
            // Lock collateral BEFORE creating asset
            Self::lock_collateral(&asset_id, &who, collateral_amount)?;
            let token_id = Self::get_and_increment_token_id();
            
            // 使用 minimal 构造函数
            let mut asset = DataAsset::minimal(who.clone(), name, description, raw_data_hash, timestamp,);
            asset.asset_id = asset_id;
            asset.token_id = token_id;
            
            Self::insert_asset(&asset_id, &asset)?;
            Self::set_token_mapping(token_id, asset_id);
            Self::initialize_certificate_trie(&asset_id);
            
            Self::deposit_event(Event::AssetRegistered { asset_id, token_id, owner: who, collateral: collateral_amount });
            Ok(())
        }

        #[pallet::call_index(1)]
        #[pallet::weight(10_000)]
        pub fn issue_certificate(
            origin: OriginFor<T>,
            asset_id: [u8; 32],
            holder: T::AccountId,
            right_type: u8,
            valid_until: Option<u64>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            
            let asset = Self::get_asset(&asset_id).ok_or(Error::<T>::AssetNotFound)?;
            ensure!(asset.owner == who, Error::<T>::NotOwner);
            ensure!(asset.is_active(), Error::<T>::AssetNotActive);
            
            // 转换 u8 到 RightType
            let right_type_enum = match right_type {
                1 => RightType::Usage,
                2 => RightType::Access,
                _ => return Err(Error::<T>::InvalidRightType.into()),
            };
            
            let token_id = Self::get_next_certificate_id(&asset_id);
            let current_time = Self::current_timestamp();
            
            // 使用 minimal 构造函数
            let mut certificate = RightToken::minimal(
                token_id,
                right_type_enum,
                holder.clone(),
                asset.owner.clone(),
                asset_id,
                current_time,
                valid_until
            );
            // certificate.token_id = RightToken::generate_token_id(asset.token_id, certificate_id);

            Self::insert_certificate(&asset_id, &certificate)?;
            Self::update_asset_certificate_root(&asset_id)?;
            
            Self::deposit_event(Event::CertificateIssued { asset_id, certificate_id: certificate.certificate_id, issuer: asset.owner.clone(), holder });
            Ok(())
        }

        #[pallet::call_index(2)]
        #[pallet::weight(10_000)]
        pub fn transfer_asset(
            origin: OriginFor<T>,
            asset_id: [u8; 32],
            new_owner: T::AccountId,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            
            let mut asset = Self::get_asset(&asset_id).ok_or(Error::<T>::AssetNotFound)?;
            ensure!(asset.owner == who, Error::<T>::NotOwner); // 只有资产所有者才能转移资产
            ensure!(!asset.is_locked(), Error::<T>::AssetLocked); // 锁定的资产不能转移
            
            let old_owner = asset.owner.clone();
            asset.owner = new_owner.clone();
            asset.nonce += 1;
            asset.transaction_count += 1;
            asset.updated_at = Self::current_timestamp();
            
            Self::insert_asset(&asset_id, &asset)?;
            
            Self::deposit_event(Event::AssetTransferred { asset_id, from: old_owner, to: new_owner });
            Ok(())
        }

        #[pallet::call_index(3)]
        #[pallet::weight(10_000)]
        pub fn revoke_certificate(
            origin: OriginFor<T>,
            asset_id: [u8; 32],
            certificate_id: [u8; 32],
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            
            let asset = Self::get_asset(&asset_id).ok_or(Error::<T>::AssetNotFound)?;
            let cert = Self::get_certificate(&asset_id, &certificate_id)
                .ok_or(Error::<T>::CertificateNotFound)?;
            
            ensure!(asset.owner == who || cert.owner == who, Error::<T>::NotOwner);
            
            Self::remove_certificate(&asset_id, &certificate_id)?;
            Self::update_asset_certificate_root(&asset_id)?;
            
            Self::deposit_event(Event::CertificateRevoked { asset_id, certificate_id });
            Ok(())
        }

        #[pallet::call_index(4)]
        #[pallet::weight(10_000)]
        pub fn lock_asset(origin: OriginFor<T>, asset_id: [u8; 32]) -> DispatchResult {
            let who = ensure_signed(origin)?;
            // let caller = Self::account_to_h160(&who);
            
            let mut asset = Self::get_asset(&asset_id).ok_or(Error::<T>::AssetNotFound)?;
            ensure!(asset.owner == who, Error::<T>::NotOwner);
            
            asset.is_locked = true;
            asset.status = AssetStatus::Locked;
            asset.updated_at = Self::current_timestamp();
            
            Self::insert_asset(&asset_id, &asset)?;
            Ok(())
        }

        #[pallet::call_index(5)]
        #[pallet::weight(10_000)]
        pub fn unlock_asset(origin: OriginFor<T>, asset_id: [u8; 32]) -> DispatchResult {
            let who = ensure_signed(origin)?;
            // let caller = Self::account_to_h160(&who);
            
            let mut asset = Self::get_asset(&asset_id).ok_or(Error::<T>::AssetNotFound)?;
            ensure!(asset.owner == who, Error::<T>::NotOwner);
            
            asset.is_locked = false;
            asset.status = AssetStatus::Active;
            asset.updated_at = Self::current_timestamp();
            
            Self::insert_asset(&asset_id, &asset)?;
            Ok(())
        }

        /// 手动罚没部分抵押品（仅限 sudo/governance）
        #[pallet::call_index(6)]
        #[pallet::weight(10_000)]
        pub fn slash_asset_collateral(origin: OriginFor<T>, asset_id: [u8; 32], slash_percentage: u8) -> DispatchResult {
            // Only sudo can slash
            ensure_root(origin)?;
            
            Self::slash_collateral(&asset_id, slash_percentage)?;
            
            Ok(())
        }
    }

    impl<T: Config> Pallet<T> {
        fn asset_trie_info() -> sp_core::storage::ChildInfo {
            sp_core::storage::ChildInfo::new_default(ASSET_TRIE_ID)
        }
        
        fn make_asset_key(asset_id: &[u8; 32]) -> Vec<u8> {
            let mut key = b"assets/".to_vec();
            key.extend_from_slice(asset_id);
            key
        }
        
        fn insert_asset(asset_id: &[u8; 32], asset: &DataAsset<T::AccountId>) -> DispatchResult {
            let child_info = Self::asset_trie_info();
            let key = Self::make_asset_key(asset_id);
            child::put(&child_info, &key, asset);
            Ok(())
        }
        
        pub fn get_asset(asset_id: &[u8; 32]) -> Option<DataAsset<T::AccountId>> {
            let child_info = Self::asset_trie_info();
            let key = Self::make_asset_key(asset_id);
            child::get::<DataAsset<T::AccountId>>(&child_info, &key)
        }
        
        pub fn get_asset_by_token_id(token_id: u32) -> Option<DataAsset<T::AccountId>> {
            let asset_id = Self::get_token_mapping(token_id)?;
            Self::get_asset(&asset_id)
        }
        
        // 获取自增的 token_id，最大2^32-1(42亿)
        fn get_and_increment_token_id() -> u32 {
            let child_info = Self::asset_trie_info();
            let key = [METADATA_PREFIX, b"next_token_id"].concat();
            
            let current = child::get::<u32>(&child_info, &key).unwrap_or(0);
            let next = current.saturating_add(1);
            child::put(&child_info, &key, &next);
            current
        }
        
        fn set_token_mapping(token_id: u32, asset_id: [u8; 32]) {
            let child_info = Self::asset_trie_info();
            let mut key = METADATA_PREFIX.to_vec();
            key.extend_from_slice(b"token_mappings/");
            key.extend_from_slice(&token_id.to_le_bytes());
            child::put(&child_info, &key, &asset_id);
        }
        
        fn get_token_mapping(token_id: u32) -> Option<[u8; 32]> {
            let child_info = Self::asset_trie_info();
            let mut key = METADATA_PREFIX.to_vec();
            key.extend_from_slice(b"token_mappings/");
            key.extend_from_slice(&token_id.to_le_bytes());
            
            child::get::<[u8; 32]>(&child_info, &key)
        }
        
        fn certificate_trie_info(asset_id: &[u8; 32]) -> sp_core::storage::ChildInfo {
            let mut key = CERTIFICATE_TRIE_PREFIX.to_vec();
            key.extend_from_slice(asset_id);
            sp_core::storage::ChildInfo::new_default(&key)
        }
        
        fn initialize_certificate_trie(asset_id: &[u8; 32]) {
            let child_info = Self::certificate_trie_info(asset_id);
            child::put(&child_info, b"_init", &[1u8]);
        }
        
        fn insert_certificate(asset_id: &[u8; 32], cert: &RightToken<T::AccountId>) -> DispatchResult {
            let child_info = Self::certificate_trie_info(asset_id);
            child::put(&child_info, &cert.certificate_id[..], cert);
            Ok(())
        }
        
        pub fn get_certificate(asset_id: &[u8; 32], cert_id: &[u8; 32]) -> Option<RightToken<T::AccountId>> {
            let child_info = Self::certificate_trie_info(asset_id);
            child::get::<RightToken<T::AccountId>>(&child_info, cert_id)
        }
                
        fn remove_certificate(asset_id: &[u8; 32], cert_id: &[u8; 32]) -> DispatchResult {
            let child_info = Self::certificate_trie_info(asset_id);
            child::kill(&child_info, cert_id);
            Ok(())
        }
        
        fn get_certificate_root(asset_id: &[u8; 32]) -> H256 {
            let child_info = Self::certificate_trie_info(asset_id);
            //child::root需要StateVersion参数
            let root_bytes = child::root(&child_info, sp_core::storage::StateVersion::V1);
            H256::from_slice(&root_bytes)
        }
        
        fn update_asset_certificate_root(asset_id: &[u8; 32]) -> DispatchResult {
            let mut asset = Self::get_asset(asset_id).ok_or(Error::<T>::AssetNotFound)?;
            let cert_root = Self::get_certificate_root(asset_id);
            asset.children_root = cert_root.into();
            asset.updated_at = Self::current_timestamp();
            Self::insert_asset(asset_id, &asset)?;
            Ok(())
        }
        
        // 机制导致不能通过遍历child trie获取某资产下的所有证书
        // pub fn get_asset_certificates(asset_id: &[u8; 32]) -> Vec<RightToken<T::AccountId>> {

        fn get_next_certificate_id(asset_id: &[u8; 32]) -> u32 {
            let child_info = Self::certificate_trie_info(asset_id);
            let key = [METADATA_PREFIX, b"next_token_id"].concat();

            let current = child::get::<u32>(&child_info, &key).unwrap_or(0);
            let next = current.saturating_add(1);
            child::put(&child_info, &key, &next);
            current
        }
        
        pub fn compute_asset_root() -> H256 {
            let child_info = Self::asset_trie_info();
            let root_bytes = child::root(&child_info, sp_core::storage::StateVersion::V1);
            H256::from_slice(&root_bytes)
        }
        
        fn current_timestamp() -> u64 {
            <pallet_timestamp::Pallet<T>>::get().saturated_into::<u64>()
        }
    }

    impl<T: Config> Pallet<T> {
        /// Get asset root from a block's digest
        pub fn get_asset_root_from_digest(digest: &sp_runtime::Digest) -> Option<H256> {
            crate::digest_item::extract_asset_root(digest)
        }
        
        /// Get asset root from current block's digest
        pub fn current_block_asset_root() -> Option<H256> {
            let digest = frame_system::Pallet::<T>::digest();
            Self::get_asset_root_from_digest(&digest)
        }
    }
}