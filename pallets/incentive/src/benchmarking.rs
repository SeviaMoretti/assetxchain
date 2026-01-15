#![cfg(feature = "runtime-benchmarks")]

use super::*;

#[allow(unused)]
use crate::Pallet as Incentive;
use frame_benchmarking::{benchmarks, account, whitelisted_caller};
use frame_system::RawOrigin;
use frame_support::traits::{Currency, Get, ReservableCurrency};
use sp_runtime::traits::{Saturating, One};

// 辅助函数：初始化激励池余额和状态
fn setup_pool<T: Config>() {
    let pool_account = incentive_pool_account::<T>();
    let initial_balance = T::InitialIncentivePool::get();
    
    // 给池子账户注入初始资金
    T::Currency::make_free_balance_be(&pool_account, initial_balance);
    
    // 模拟初始释放逻辑（如 on_runtime_upgrade 所做的那样）
    let release_ratio = T::DynamicReleaseRatio::get();
    let initial_release = release_ratio * initial_balance;
    let locked_amount = initial_balance.saturating_sub(initial_release);
    
    // 锁定部分资金
    let _ = T::Currency::reserve(&pool_account, locked_amount);
    
    IncentivePoolReleased::<T>::put(initial_release);
    IncentivePoolReserved::<T>::put(locked_amount);
}

benchmarks! {
    // 1. 测试 trigger_dynamic_release (手动触发释放)
    trigger_dynamic_release {
        setup_pool::<T>();
        // 确保还有剩余可释放金额
        let reserved = IncentivePoolReserved::<T>::get();
        assert!(reserved > 0u32.into());
    }: _(RawOrigin::Root)
    verify {
        // 验证已释放总额增加
        assert!(IncentivePoolReleased::<T>::get() > 0u32.into());
    }

    // 2. 测试 distribute_quality_data_reward (优质数据奖励)
    distribute_quality_data_reward {
        setup_pool::<T>();
        let asset_id: AssetId = [1u8; 32];
        let threshold = T::QualityDataTradeThreshold::get();
        
        // 准备数据：手动插入交易笔数达到阈值
        Asset30dTradeCount::<T>::insert(&asset_id, threshold);
        
        // 注意：这里依赖 DataAssetProvider 能返回一个有效的 Owner。
        // 在 Benchmark 环境下，通常 mock 或真实配置的 provider 需要能处理 [1u8; 32]
    }: _(RawOrigin::Root, asset_id)
    verify {
        // 验证已使用额度增加（假设奖励金额 > 0）
        assert!(IncentivePoolUsed::<T>::get() > 0u32.into());
    }

    // 3. 测试 register_market_monthly_volume (登记市场交易额)
    register_market_monthly_volume {
        let caller: T::AccountId = whitelisted_caller();
        let market_id: [u8; 32] = [2u8; 32];
        let volume: BalanceOf<T> = 10_000u32.into();
    }: _(RawOrigin::Signed(caller), market_id, volume)
    verify {
        assert_eq!(MarketMonthlyVolume::<T>::get(&market_id), volume);
    }

    // 4. 测试 register_voting_weight (登记投票权重)
    register_voting_weight {
        let voter: T::AccountId = account("voter", 0, 0);
        let weight: BalanceOf<T> = 5_000u32.into();
    }: _(RawOrigin::Root, voter.clone(), weight)
    verify {
        assert_eq!(GovernanceVotingWeight::<T>::get(&voter), weight);
    }

    impl_benchmark_test_suite!(Incentive, crate::mock::new_test_ext(), crate::mock::Test);
}