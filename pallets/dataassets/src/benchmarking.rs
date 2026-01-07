use super::*;


#[allow(unused)]
use crate::Pallet as DataAssets;
use frame_benchmarking::v2::*;
use frame_system::RawOrigin;
use sp_core::H256;
use frame_support::traits::{Currency, Get};
use sp_runtime::traits::Saturating;
use alloc::vec;
use crate::Event;
use frame_system::Config as SystemConfig;

type RuntimeEventOf<T> = <T as SystemConfig>::RuntimeEvent;

fn setup_user<T: Config>(caller: T::AccountId) {
    let amount = T::MaxCollateral::get();
    let extra = BalanceOf::<T>::from(1000u32); 
    let total = amount.saturating_add(extra);
    T::Currency::make_free_balance_be(&caller, total);
}

#[benchmarks]
mod benchmarks {
    use super::*;

    #[benchmark]
    fn register_asset(
        n: Linear<0, { T::MaxNameLength::get() }>,
        d: Linear<0, { T::MaxDescriptionLength::get() }>,
    ){
        let caller: T::AccountId = whitelisted_caller();

        setup_user::<T>(caller.clone());
        
        // let name = vec![0u8; n as usize];
        // let description = vec![0u8; d as usize];
        let name = vec![0u8; T::MaxNameLength::get() as usize];
        let description = vec![0u8; T::MaxDescriptionLength::get() as usize];
        let raw_data_hash = H256::repeat_byte(1);
        let data_size_bytes = 1024 * 1024; // 1MB

        #[extrinsic_call]
        register_asset(RawOrigin::Signed(caller.clone()), name, description, raw_data_hash, data_size_bytes);
    }

    #[benchmark]
    fn transfer_asset() {
        let caller: T::AccountId = whitelisted_caller();
        frame_system::Pallet::<T>::reset_events();
        setup_user::<T>(caller.clone());
        
        // let name = vec![0u8; n as usize];
        // let description = vec![0u8; d as usize];
        let name = vec![0u8; T::MaxNameLength::get() as usize];
        let description = vec![0u8; T::MaxDescriptionLength::get() as usize];
        let raw_data_hash = H256::repeat_byte(1);
        let data_size_bytes = 1024 * 1024; // 1MB

        // 注册资产
        let _ = DataAssets::<T>::register_asset(
            RawOrigin::Signed(caller.clone()).into(),
            name,
            description,
            raw_data_hash,
            data_size_bytes
        );
        let events = frame_system::Pallet::<T>::events();
        let last_event = events.last().expect("AssetRegistered event expected");

        let asset_id = crate::LAST_ASSET_ID.with(|v| {
            v.borrow().expect("asset id must be recorded")
        });
        let new_owner: T::AccountId = account("new_owner", 0, 0);

        #[extrinsic_call]
        transfer_asset(RawOrigin::Signed(caller), asset_id, new_owner);
    }

    impl_benchmark_test_suite!(
        Pallet,
        crate::mock::new_test_ext(),
        crate::mock::Test,
    );
}