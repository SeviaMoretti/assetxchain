#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

// 权重定义
pub mod weights {
    use frame_support::weights::Weight;

    pub trait WeightInfo {
        fn pledge() -> Weight;
        fn unbond() -> Weight;
    }

    // 占位符实现
    impl WeightInfo for () {
        fn pledge() -> Weight { Weight::from_parts(10_000_000, 0) }
        fn unbond() -> Weight { Weight::from_parts(10_000_000, 0) }
    }
}

#[frame_support::pallet]
pub mod pallet {
    use super::weights::WeightInfo;
    use frame_support::{
        pallet_prelude::*,
        traits::{Currency, ReservableCurrency, Get, ExistenceRequirement, Imbalance},
        transactional,
    };
    use frame_system::pallet_prelude::*;
    use sp_runtime::{
        traits::{Zero, CheckedAdd, CheckedSub, SaturatedConversion, Bounded, AccountIdConversion},
        DispatchError, ArithmeticError,
    };
    use scale_info::TypeInfo;
    use core::convert::TryInto;
    use codec::{Encode, Decode, MaxEncodedLen, DecodeWithMemTracking};

    /// 货币类型的别名
    type BalanceOf<T> =
        <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

    /// 惩罚类型，用于决定资金分配比例
    #[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen, DecodeWithMemTracking)]
    pub enum SlashType {
        /// 50% 销毁, 50% 进激励池
        HeavyViolation,
        /// 30% 销毁, 20% 补偿用户, 50% 激励池
        LightViolation,
        /// 市场运营者重度违规：50% 销毁, 50% 补偿用户
        MarketOperatorHeavy,
        /// IPFS 服务提供者重度违规：50% 销毁, 50% 转入 IPFS 存储池
        IpfsProviderHeavy,
    }

    /// 质押角色枚举
    #[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen, DecodeWithMemTracking)]
    pub enum CollateralRole {
        DataCreator,        // 数据创建者
        MarketOperator,     // 市场运营者
        IpfsProvider,       // IPFS服务提供者
        GovernancePledge,   // 治理参与者
    }

    /// 质押详细信息结构体
    #[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen, Default, DecodeWithMemTracking)]
    pub struct CollateralInfo<BlockNumber, Balance> {
        pub amount: Balance,                        // 当前质押金额
        pub start_block: BlockNumber,               // 质押起始区块
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::config]
    pub trait Config: frame_system::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
        
        /// 使用 ReservableCurrency trait 来管理质押的锁定
        type Currency: ReservableCurrency<Self::AccountId>; 

        /// 定义各种角色的最小质押金额
        #[pallet::constant]
        type MinMarketOperatorCollateral: Get<BalanceOf<Self>>;
        #[pallet::constant]
        type MinIpfsProviderCollateral: Get<BalanceOf<Self>>;
        #[pallet::constant]
        type MinGovernancePledge: Get<BalanceOf<Self>>;

        /// 专门用于处理惩罚（Slash）和奖励（Reward）的账户ID
        #[pallet::constant]
        type IncentivePoolAccount: Get<Self::AccountId>;
        /// 用于通缩机制中的销毁（黑洞账户）
        #[pallet::constant]
        type DestructionAccount: Get<Self::AccountId>; 
        /// IPFS 存储费用池（一个资产一个池子）
        #[pallet::constant]
        type IpfsPoolAccount: Get<Self::AccountId>;
        /// 用户补偿池 (用于 MarketOperator 违规)
        #[pallet::constant]
        type CompensationPoolAccount: Get<Self::AccountId>;

        /// Pallet ID 用于派生账户
        #[pallet::constant]
        type PalletId: Get<frame_support::PalletId>;

        /// Weight information
        type WeightInfo: WeightInfo;
    }

    // 存储所有定制化质押角色的质押信息
    #[pallet::storage]
    #[pallet::getter(fn collateral_data)]
    pub type CollateralData<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat, T::AccountId,
        Blake2_128Concat, CollateralRole,
        CollateralInfo<BlockNumberFor<T>, BalanceOf<T>>,
        ValueQuery,
    >;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// 质押成功
        Pledged { who: T::AccountId, role: CollateralRole, amount: BalanceOf<T> },
        /// 解除质押成功
        Unbonded { who: T::AccountId, role: CollateralRole, amount: BalanceOf<T> },
        /// 质押被惩罚并分配
        SlashedAndDistributed { 
            who: T::AccountId, 
            role: CollateralRole, 
            slashed_amount: BalanceOf<T>, 
            burn_amount: BalanceOf<T>, 
            incentive_amount: BalanceOf<T> 
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        /// 质押金额不足最小要求
        InsufficientCollateralAmount,
        /// 质押信息未找到
        CollateralNotFound,
        /// 质押金额为零
        AmountIsZero,
        /// 质押条件尚未满足，无法释放
        CollateralNotReadyForRelease,
        /// 角色不支持此操作
        UnsupportedRole,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// 通用质押函数
        #[pallet::call_index(0)]
        #[pallet::weight(T::WeightInfo::pledge())]
        #[transactional]
        // #[transactional]确保质押操作是原子的，要么全部成功，要么全部失败
        pub fn pledge(
            origin: OriginFor<T>,
            role: CollateralRole,
            amount: BalanceOf<T>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            ensure!(!amount.is_zero(), Error::<T>::AmountIsZero);

            // 1. 检查最小质押要求
            Self::ensure_min_collateral(&role, amount)?;

            // 2. 锁定（保留）用户的资金
            T::Currency::reserve(&who, amount)?;

            // 3. 更新或创建质押信息
            CollateralData::<T>::try_mutate(
                &who,
                &role,
                |collateral_info| -> DispatchResult {
                    collateral_info.amount = collateral_info.amount.checked_add(&amount)
                        .ok_or(ArithmeticError::Overflow)?;

                    if collateral_info.start_block.is_zero() {
                        collateral_info.start_block = frame_system::Pallet::<T>::block_number();
                    }
                    Ok(())
                }
            )?;

            Self::deposit_event(Event::Pledged { who, role, amount });
            Ok(())
        }

        /// 通用解除质押/释放函数
        #[pallet::call_index(1)]
        #[pallet::weight(T::WeightInfo::unbond())]
        #[transactional]
        pub fn unbond(
            origin: OriginFor<T>,
            role: CollateralRole,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            
            // 1. 检查质押信息是否存在
            ensure!(CollateralData::<T>::contains_key(&who, &role), Error::<T>::CollateralNotFound);
            let collateral_info = CollateralData::<T>::get(&who, &role);

            // 2. 检查并计算可释放金额和剩余金额
            let (releasable_amount, remaining_amount) = Self::get_releasable_amount(&role, &collateral_info)?;

            ensure!(!releasable_amount.is_zero(), Error::<T>::CollateralNotReadyForRelease);

            // 3. 将资金从保留状态转移到自由状态
            T::Currency::unreserve(&who, releasable_amount);
            
            // 4. 更新存储状态
            if !remaining_amount.is_zero() {
                CollateralData::<T>::mutate(&who, &role, |info| {
                    info.amount = remaining_amount;
                });
            } else {
                // 如果全部释放，则移除存储项
                CollateralData::<T>::remove(&who, &role);
            }

            Self::deposit_event(Event::Unbonded { who, role, amount: releasable_amount });
            Ok(())
        }
    }

    /// 辅助函数
    impl<T: Config> Pallet<T> {
        
        /// 检查最小质押要求
        fn ensure_min_collateral(role: &CollateralRole, amount: BalanceOf<T>) -> DispatchResult {
            let min_amount = match role {
                CollateralRole::MarketOperator => T::MinMarketOperatorCollateral::get(),
                CollateralRole::IpfsProvider => T::MinIpfsProviderCollateral::get(),
                CollateralRole::GovernancePledge => T::MinGovernancePledge::get(),
                // 数据创建者的基础质押在业务 Pallet 中处理
                _ => BalanceOf::<T>::zero(), 
            };
            
            if !min_amount.is_zero() {
                ensure!(amount >= min_amount, Error::<T>::InsufficientCollateralAmount);
            }
            Ok(())
        }

        /// 计算可释放金额和剩余金额
        fn get_releasable_amount(
            role: &CollateralRole,
            info: &CollateralInfo<BlockNumberFor<T>, BalanceOf<T>>,
        ) -> Result<(BalanceOf<T>, BalanceOf<T>), DispatchError> {
            
            match role {
                CollateralRole::DataCreator => {
                    // 检查是否通过了 90 天长期可用验证
                    let ninety_days_u32 = 90u32 * 24 * 60;
                    let ninety_days = BlockNumberFor::<T>::from(ninety_days_u32); // 假设计算单位

                    if frame_system::Pallet::<T>::block_number() > info.start_block.checked_add(&ninety_days).unwrap_or(Bounded::max_value()) {
                        Ok((info.amount, BalanceOf::<T>::zero()))
                    } else {
                        Err(Error::<T>::CollateralNotReadyForRelease.into())
                    }
                },
                CollateralRole::MarketOperator => {
                    // 运营满 2 年后释放
                    let two_years_u32 = 365 * 2 * 24 * 60u32;
                    let two_years = BlockNumberFor::<T>::from(two_years_u32);
                    if frame_system::Pallet::<T>::block_number() > info.start_block.checked_add(&two_years).unwrap_or(Bounded::max_value()) {
                        Ok((info.amount, BalanceOf::<T>::zero()))
                    } else {
                        Err(Error::<T>::CollateralNotReadyForRelease.into())
                    }
                },
                _ => {
                    // 对于其他角色，简单锁定 7 天后可释放
                    let lock_period_u32 = 7u32 * 24 * 60;
                    let lock_period = BlockNumberFor::<T>::from(lock_period_u32);
                    if frame_system::Pallet::<T>::block_number() > info.start_block.checked_add(&lock_period).unwrap_or(Bounded::max_value()) {
                        Ok((info.amount, BalanceOf::<T>::zero()))
                    } else {
                        Err(Error::<T>::CollateralNotReadyForRelease.into())
                    }
                }
            }
        }
        
        /// 执行惩罚和资金分配
        #[transactional]
        pub fn slash_and_distribute(
            who: &T::AccountId,
            role: CollateralRole,
            slash_amount: BalanceOf<T>,
            slash_type: SlashType,
        ) -> Result<BalanceOf<T>, DispatchError> {
            ensure!(!slash_amount.is_zero(), Error::<T>::AmountIsZero);
            
            // 1. 从用户的保留余额中扣除
            let (slashed_imbalance, _) = T::Currency::slash_reserved(who, slash_amount);
            let slashed_amount = slashed_imbalance.peek();

            if slashed_amount.is_zero() {
                return Ok(BalanceOf::<T>::zero());
            }

            // 2. 根据惩罚类型确定分配比例
            let (burn_ratio, incentive_ratio, compensation_ratio, ipfs_ratio) = match slash_type {
                SlashType::HeavyViolation => (50, 50, 0, 0),
                SlashType::LightViolation => (30, 70, 0, 0),
                SlashType::MarketOperatorHeavy => (50, 0, 50, 0),
                SlashType::IpfsProviderHeavy => (50, 0, 0, 50),
            };

            // 3. 计算分配金额
            let total_u128: u128 = slashed_amount.saturated_into();
            
            let burn_amount: BalanceOf<T> = (total_u128 * burn_ratio as u128 / 100).saturated_into();
            let incentive_amount: BalanceOf<T> = (total_u128 * incentive_ratio as u128 / 100).saturated_into();
            let compensation_amount: BalanceOf<T> = (total_u128 * compensation_ratio as u128 / 100).saturated_into();
            let ipfs_amount: BalanceOf<T> = (total_u128 * ipfs_ratio as u128 / 100).saturated_into();
            
            let remaining = slashed_amount
                .checked_sub(&burn_amount)
                .and_then(|r| r.checked_sub(&incentive_amount))
                .and_then(|r| r.checked_sub(&compensation_amount))
                .and_then(|r| r.checked_sub(&ipfs_amount))
                .unwrap_or_else(|| BalanceOf::<T>::zero());
            
            let final_incentive_amount = incentive_amount.checked_add(&remaining).unwrap_or(incentive_amount);

            // 4. 执行资金转移 - 需要将 slashed_imbalance 分解并分配到各个账户
            // 由于 Currency::slash_reserved 返回 NegativeImbalance，我们需要处理这个不平衡
            // 这里简化处理：直接 drop 不平衡（相当于销毁），然后从其他地方转移资金
            // 在实际实现中，您可能需要更复杂的资金分配逻辑
            
            drop(slashed_imbalance); // 销毁不平衡
            
            // 注意：这里需要从 pallet 账户转移资金到各个目标账户
            // 但需要确保 pallet 账户有足够的资金
            let pallet_account = Self::account_id();
            
            // 从 pallet 账户转移资金到各个池子
            // 销毁 (转入黑洞)
            if !burn_amount.is_zero() {
                T::Currency::transfer(&pallet_account, &T::DestructionAccount::get(), burn_amount, ExistenceRequirement::KeepAlive)?;
            }
            
            // 激励池
            if !final_incentive_amount.is_zero() {
                T::Currency::transfer(&pallet_account, &T::IncentivePoolAccount::get(), final_incentive_amount, ExistenceRequirement::KeepAlive)?;
            }
            
            // 补偿池
            if !compensation_amount.is_zero() {
                T::Currency::transfer(&pallet_account, &T::CompensationPoolAccount::get(), compensation_amount, ExistenceRequirement::KeepAlive)?;
            }
            
            // IPFS 存储池
            if !ipfs_amount.is_zero() {
                T::Currency::transfer(&pallet_account, &T::IpfsPoolAccount::get(), ipfs_amount, ExistenceRequirement::KeepAlive)?;
            }

            // 5. 更新存储中的质押金额
            CollateralData::<T>::mutate(who, &role, |info| {
                info.amount = info.amount.checked_sub(&slashed_amount).unwrap_or_else(|| BalanceOf::<T>::zero());
                if info.amount.is_zero() {
                    CollateralData::<T>::remove(who, &role);
                }
            });

            Self::deposit_event(Event::SlashedAndDistributed { 
                who: who.clone(), 
                role,
                slashed_amount, 
                burn_amount, 
                incentive_amount: final_incentive_amount,
            });

            Ok(slashed_amount)
        }

        // 获取 Pallet 自己的账户 ID
        pub fn account_id() -> T::AccountId {
            T::PalletId::get().into_account_truncating()
        }
    }
}