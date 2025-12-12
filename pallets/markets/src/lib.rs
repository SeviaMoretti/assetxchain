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

    use pallet_contracts::{CollectEvents, DebugInfo, Determinism, chain_extension::ReturnFlags};

    use codec::{Encode, Decode, MaxEncodedLen, DecodeWithMemTracking};
    
    // 计算 ink! trait 中 is_assetx_market 的 selector
    // 此处假设为 [0x2A, 0x5F, 0x57, 0x6B]，实际开发需用 cargo-contract 计算
    const SELECTOR_IS_MARKET: [u8; 4] = [0x2A, 0x5F, 0x57, 0x6B];

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
    pub trait Config: frame_system::Config + pallet_contracts::Config {
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
        /// 市场验证失败
        MarketVerificationFailed,
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
        #[pallet::weight(Weight::from_parts(10_000, 0))] // 修复权重警告
        pub fn register_market(
            origin: OriginFor<T>,
            contract_address: T::AccountId,
            asset_type: MarketAssetType,
        ) -> DispatchResult {
            let creator = ensure_signed(origin)?;

            // 确保该合约地址没有被注册过
            ensure!(!RegisteredMarkets::<T>::contains_key(&contract_address), Error::<T>::MarketAlreadyExists);
            let asset_type_for_event = asset_type.clone();
            
            // 调用合约的is_assetx_market方法验证市场是否符合标准
            let input_data = SELECTOR_IS_MARKET.to_vec();
            let gas_limit = Weight::from_parts(5_000_000_000, 256 * 1024);

            let result = pallet_contracts::Pallet::<T>::bare_call(
                creator.clone(),          // 模拟调用者
                contract_address.clone(), // 目标合约
                0u32.into(),              // 转账 0
                gas_limit,
                None,
                input_data,
                DebugInfo::Skip,          // 改为 DebugInfo 类型
                CollectEvents::Skip,
                Determinism::Enforced,    // 改为 Enforced
            );

            // 检查返回值是否为 true (ink! bool true = 0x01)
            let verified = match result.result {
                Ok(retval) => {
                     !retval.flags.contains(ReturnFlags::REVERT) && 
                     retval.data.len() >= 1 && 
                     retval.data[0] == 1
                },
                Err(_) => false,
            };

            ensure!(verified, Error::<T>::MarketVerificationFailed);

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
        #[pallet::weight(Weight::from_parts(10_000, 0))] // 修复权重警告
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