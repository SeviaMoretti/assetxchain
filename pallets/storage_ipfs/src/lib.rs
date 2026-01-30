#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::pallet_prelude::*;
    use frame_system::pallet_prelude::*;
    use sp_std::vec::Vec;
    use sp_core::H256;
    
    // 引入依赖模块的类型
    use pallet_collaterals::{CollateralRole, Pallet as CollateralPallet};
    use pallet_shared_traits::{DataAssetInternal, EncryptionInfo};

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::config]
    pub trait Config: frame_system::Config + pallet_collaterals::Config + pallet_dataassets::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
        
        /// 资产处理接口，用于调用 pallet-dataassets
        type AssetHandler: DataAssetInternal<Self::AccountId, BalanceOf<Self>>;

        /// 存储证明的有效周期（以区块数为单位）
        #[pallet::constant]
        type ProofPeriod: Get<BlockNumberFor<Self>>;
    }

    type BalanceOf<T> = <<T as pallet_collaterals::Config>::Currency as frame_support::traits::Currency<<T as frame_system::Config>::AccountId>>::Balance;

    /// 存储提供者信息
    #[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
    pub struct ProviderInfo<BlockNumber> {
        pub endpoint: BoundedVec<u8, ConstU32<128>>, // IPFS Multiaddr
        pub registered_at: BlockNumber,
        pub is_active: bool,
    }

    /// 存储证明记录
    #[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
    pub struct StorageProof<BlockNumber> {
        pub last_proof_block: BlockNumber,
        pub proof_hash: H256,
    }

    #[pallet::storage]
    #[pallet::getter(fn providers)]
    pub type Providers<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        ProviderInfo<BlockNumberFor<T>>,
        OptionQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn storage_proofs)]
    pub type StorageProofs<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat, [u8; 32], // asset_id
        Blake2_128Concat, T::AccountId, // provider
        StorageProof<BlockNumberFor<T>>,
        OptionQuery,
    >;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        ProviderRegistered { who: T::AccountId, endpoint: Vec<u8> },
        ProofSubmitted { asset_id: [u8; 32], provider: T::AccountId },
    }

    #[pallet::error]
    pub enum Error<T> {
        NotAProvider,
        ProviderAlreadyExists,
        InvalidEndpoint,
        AssetNotRegistered,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// 注册成为 IPFS 存储服务商
        #[pallet::call_index(0)]
        #[pallet::weight(10_000)]
        pub fn register_provider(
            origin: OriginFor<T>,
            endpoint: Vec<u8>,
            pledge_amount: BalanceOf<T>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            
            ensure!(!Providers::<T>::contains_key(&who), Error::<T>::ProviderAlreadyExists);
            
            let bounded_endpoint: BoundedVec<u8, ConstU32<128>> = 
                endpoint.clone().try_into().map_err(|_| Error::<T>::InvalidEndpoint)?;

            // 调用 pallet-collaterals 执行质押逻辑
            // 注意：需要 pallet-collaterals 的 internal_pledge 是 pub 的
            CollateralPallet::<T>::internal_pledge(
                &who, 
                CollateralRole::IpfsProvider, 
                pledge_amount
            )?;

            Providers::<T>::insert(&who, ProviderInfo {
                endpoint: bounded_endpoint,
                registered_at: frame_system::Pallet::<T>::block_number(),
                is_active: true,
            });

            Self::deposit_event(Event::ProviderRegistered { who, endpoint });
            Ok(())
        }

        /// 作为代理入口注册 IPFS 资产
        #[pallet::call_index(1)]
        #[pallet::weight(10_000)]
        pub fn register_ipfs_asset(
            origin: OriginFor<T>,
            name: Vec<u8>,
            description: Vec<u8>,
            metadata_cid: Vec<u8>,
            raw_data_hash: H256,
            data_size_bytes: u64,
            encryption_info: EncryptionInfo,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;

            // 1. 基础验证（可选，如验证 CID 格式）
            
            // 2. 调用 pallet-dataassets 进行核心资产注册逻辑
            T::AssetHandler::register_asset(
                who,
                name,
                description,
                raw_data_hash,
                data_size_bytes,
                metadata_cid,
                encryption_info,
            )?;

            Ok(())
        }

        /// 存储提供者提交存储证明
        #[pallet::call_index(2)]
        #[pallet::weight(10_000)]
        pub fn submit_storage_proof(
            origin: OriginFor<T>,
            asset_id: [u8; 32],
            proof_hash: H256,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            
            ensure!(Providers::<T>::contains_key(&who), Error::<T>::NotAProvider);
            
            // 记录证明
            StorageProofs::<T>::insert(asset_id, &who, StorageProof {
                last_proof_block: frame_system::Pallet::<T>::block_number(),
                proof_hash,
            });

            Self::deposit_event(Event::ProofSubmitted { asset_id, provider: who });
            Ok(())
        }
    }
}