// 资产交易市场模块
// 与pallet-contracts交互，实现资产交易市场
// 市场：交易模式：订单薄、拍卖等
//      准入规则
//      手续费
//      资产类型：数据元证、权证
// 
// pallet存储：市场注册表（创建者、合约账户）
// 市场相关合约-合约账户
#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

mod original_lib;

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::pallet_prelude::*;
    use frame_system::pallet_prelude::*;

    use codec::{Encode, Decode, MaxEncodedLen, DecodeWithMemTracking};
    
    #[pallet::pallet]
    pub struct Pallet<T>(_);
    
    // 定义市场支持的资产类型，另一种资产类型在准入规则中限制、定义
    #[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen, DecodeWithMemTracking)]
    pub enum MarketAssetType {
        DataAsset,      // 交易元证 (Assets)
        Certificate,    // 交易权证 (Certificates)
    }

    // 市场元数据
    #[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen, DecodeWithMemTracking)]
    pub struct MarketRegistryInfo<AccountId> {
        pub creator: AccountId,         // 创建者
        pub contract_address: AccountId,// 智能合约地址 (Ink!合约部署后的地址)
        pub asset_type: MarketAssetType,// 交易资产类型
        pub status: MarketStatus,       // Active, Suspended
    }

    #[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen, DecodeWithMemTracking)]
    pub enum MarketStatus {
        Active,
        Inactive,
    }

    #[pallet::config]
    pub trait Config: frame_system::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
    }

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        MarketRegistered {
            creator: T::AccountId,
            contract_address: T::AccountId,
            asset_type: MarketAssetType,
        },
        MarketUnregistered {
            contract_address: T::AccountId,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        /// 市场已存在
        MarketAlreadyExists,
        /// 市场不存在
        MarketNotFound,
        /// 不是市场所有者
        NotOwner,
    }

    #[pallet::storage]
    #[pallet::getter(fn registered_markets)]
    // 使用 ContractAddress 作为 Key，确保一个合约对应一个市场记录
    pub type RegisteredMarkets<T: Config> = StorageMap<
        _, 
        Blake2_128Concat, 
        T::AccountId, // Contract Address
        MarketRegistryInfo<T::AccountId>
    >;

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// 注册一个新市场
        /// 用户先部署智能合约，获得 contract_address，然后调用此函数进行注册
        #[pallet::call_index(0)]
        #[pallet::weight(10_000)]
        pub fn register_market(
            origin: OriginFor<T>,
            contract_address: T::AccountId,
            asset_type: MarketAssetType,
        ) -> DispatchResult {
            let creator = ensure_signed(origin)?;

            // 确保该合约地址没有被注册过
            ensure!(!RegisteredMarkets::<T>::contains_key(&contract_address), Error::<T>::MarketAlreadyExists);
            let asset_type_for_event = asset_type.clone();
            // 这里添加逻辑验证 contract_address 是否真的是一个合约 (依赖 pallet-contracts)
            // 验证调用者是否拥有该合约的 owner 权限

            let info = MarketRegistryInfo {
                creator: creator.clone(),
                contract_address: contract_address.clone(),
                asset_type,
                status: MarketStatus::Active,
            };

            RegisteredMarkets::<T>::insert(&contract_address, info);

            Self::deposit_event(Event::MarketRegistered { creator, contract_address, asset_type: asset_type_for_event });
            Ok(())
        }

        /// 注销市场
        #[pallet::call_index(1)]
        #[pallet::weight(10_000)]
        pub fn unregister_market(
            origin: OriginFor<T>,
            contract_address: T::AccountId,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            
            let market = RegisteredMarkets::<T>::get(&contract_address).ok_or(Error::<T>::MarketNotFound)?;
            ensure!(market.creator == who, Error::<T>::NotOwner);

            RegisteredMarkets::<T>::remove(&contract_address);
            
            Self::deposit_event(Event::MarketUnregistered { contract_address });
            Ok(())
        }
    }
}