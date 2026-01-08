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
    
    /// 函数选择器：对应ink!合约的is_assetx_market()方法
    /// 生成方式：在合约项目中执行 `cargo contract metadata --json | jq '.V1.spec.messages[] | select(.name == "is_assetx_market") | .selector'`
    /// 生产环境必须替换为实际生成的选择器，否则会导致调用失败
    // ###应该将Selector放入 Config，在编译后的合约的metadata.json(target/ink/market_orderbook/market_orderbook.json)中查看
    const SELECTOR_IS_MARKET: [u8; 4] = [0x26, 0x3e, 0x53, 0x34];
    // 会添加多个Selector，约束市场创建者必须实现某些方法

    #[pallet::pallet]
    pub struct Pallet<T>(_);
    
    // 多源同构数据市场，市场类型得展示出来，只在智能合约里面就很鸡肋

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
            // 应该估算合约调用所需gas（避免硬编码导致的gas不足或浪费）
            let gas_limit = Weight::from_parts(5_000_000_000, 256 * 1024);

            let result = pallet_contracts::Pallet::<T>::bare_call(
                creator.clone(),          // 调用者账号（市场创建者）
                contract_address.clone(), // 目标合约地址（要验证的合约）
                0u32.into(),              // 随调用转账的金额（这里为 0，调用方法不转账）
                gas_limit,                // 调用允许消耗的最大 gas（防止无限循环等问题）
                None,                     // 盐值（用于创建合约时的确定性地址，调用已部署合约，None）
                input_data,               // 输入数据（包含函数选择器，用于指定调用合约的哪个方法）
                DebugInfo::Skip,          // 是否收集调试信息（跳过）
                CollectEvents::Skip,      // 是否收集合约触发的事件（跳过）
                Determinism::Enforced,    // 是否强制确定性执行（确保调用结果可复现）
            );

            // 检查返回值是否为 true (ink! bool true = 0x01)
            // 验证调用结果（细化错误处理，明确失败原因）
            let verified = match result.result {
                Ok(retval) => {
                    // 检查是否发生了 Revert
                    if retval.flags.contains(ReturnFlags::REVERT) {
                        false
                    } else {
                        // 将返回数据解码为 Result<bool, _>
                        let decoded_result: Result<Result<bool, u8>, _> = Decode::decode(&mut &retval.data[..]);
                        
                        match decoded_result {
                            // 外层Ok代表解码成功，内层Ok(true)代表合约返回了True
                            Ok(Ok(true)) => true,
                            _ => false,
                        }
                    }
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