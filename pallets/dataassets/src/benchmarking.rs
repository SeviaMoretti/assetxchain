//! Benchmarking setup for pallet-dataassets

use super::*;
use crate::Pallet as DataAssets;
use frame_benchmarking::v2::*;
use frame_support::traits::{ Currency, Get };
use frame_system::RawOrigin;
use sp_core::H256;
use sp_runtime::traits::{ Saturating, SaturatedConversion };
use sp_std::vec;

// 为基准测试创建账户并提供资金
fn create_funded_account<T: Config>(name: &'static str, index: u32) -> T::AccountId {
    let account: T::AccountId = frame_benchmarking::account(name, index, 0);
    let balance = T::Currency::minimum_balance() * 1000u32.into();
    T::Currency::make_free_balance_be(&account, balance);
    account
}

#[benchmarks]
mod benchmarks {
    use super::*;

    #[benchmark]
    fn register_asset() {
        // 参数
        let caller = create_funded_account::<T>("caller", 0);
        let name = vec![b'T'; T::MaxNameLength::get() as usize];
        let description = vec![b'D'; T::MaxDescriptionLength::get() as usize];
        let raw_data_hash = H256::repeat_byte(0x01);
        let data_size_bytes = 1024 * 1024; // 1 MB

        // 确保有足够的质押金
        let collateral = T::BaseCollateral::get()
            .saturating_add(T::CollateralPerMB::get());
        T::Currency::make_free_balance_be(&caller, collateral * 10u32.into());

        #[extrinsic_call]
        register_asset(
            RawOrigin::Signed(caller.clone()),
            name,
            description,
            raw_data_hash,
            data_size_bytes,
        );

        // 验证
        assert!(DataAssets::<T>::get_asset(&[0u8; 32]).is_some() 
            || frame_system::Pallet::<T>::events().len() > 0);
    }

    #[benchmark]
    fn issue_certificate() {
        // 先注册资产
        let owner = create_funded_account::<T>("owner", 0);
        let holder = create_funded_account::<T>("holder", 1);
        
        let name = b"Test Asset".to_vec();
        let description = b"Test Description".to_vec();
        let raw_data_hash = H256::repeat_byte(0x01);
        let data_size_bytes = 1024 * 1024;

        let collateral = T::BaseCollateral::get()
            .saturating_add(T::CollateralPerMB::get());
        T::Currency::make_free_balance_be(&owner, collateral * 10u32.into());

        // 注册资产
        assert!(DataAssets::<T>::register_asset(
            RawOrigin::Signed(owner.clone()).into(),
            name.clone(),
            description.clone(),
            raw_data_hash,
            data_size_bytes,
        ).is_ok());

        // 获取生成的 asset_id
        let timestamp = <pallet_timestamp::Pallet<T>>::get().saturated_into::<u64>();
        let asset_id = crate::types::DataAsset::generate_asset_id(&owner, timestamp, &raw_data_hash);

        #[extrinsic_call]
        issue_certificate(
            RawOrigin::Signed(owner.clone()),
            asset_id,
            holder,
            1u8, // Usage right
            None, // No expiration
        );

        // 验证
        assert!(frame_system::Pallet::<T>::events().len() > 0);
    }

    #[benchmark]
    fn transfer_asset() {
        // 设置
        let owner = create_funded_account::<T>("owner", 0);
        let new_owner = create_funded_account::<T>("new_owner", 1);
        
        let name = b"Test Asset".to_vec();
        let description = b"Test Description".to_vec();
        let raw_data_hash = H256::repeat_byte(0x01);
        let data_size_bytes = 1024 * 1024;

        let collateral = T::BaseCollateral::get()
            .saturating_add(T::CollateralPerMB::get());
        T::Currency::make_free_balance_be(&owner, collateral * 10u32.into());

        assert!(DataAssets::<T>::register_asset(
            RawOrigin::Signed(owner.clone()).into(),
            name,
            description,
            raw_data_hash,
            data_size_bytes,
        ).is_ok());

        let timestamp = <pallet_timestamp::Pallet<T>>::get().saturated_into::<u64>();
        let asset_id = crate::types::DataAsset::generate_asset_id(&owner, timestamp, &raw_data_hash);

        #[extrinsic_call]
        transfer_asset(
            RawOrigin::Signed(owner.clone()),
            asset_id,
            new_owner,
        );

        // 验证
        assert!(frame_system::Pallet::<T>::events().len() > 0);
    }

    #[benchmark]
    fn revoke_certificate() {
        // 设置
        let owner = create_funded_account::<T>("owner", 0);
        let holder = create_funded_account::<T>("holder", 1);
        
        let name = b"Test Asset".to_vec();
        let description = b"Test Description".to_vec();
        let raw_data_hash = H256::repeat_byte(0x01);
        let data_size_bytes = 1024 * 1024;

        let collateral = T::BaseCollateral::get()
            .saturating_add(T::CollateralPerMB::get());
        T::Currency::make_free_balance_be(&owner, collateral * 10u32.into());

        assert!(DataAssets::<T>::register_asset(
            RawOrigin::Signed(owner.clone()).into(),
            name,
            description,
            raw_data_hash,
            data_size_bytes,
        ).is_ok());

        let timestamp = <pallet_timestamp::Pallet<T>>::get().saturated_into::<u64>();
        let asset_id = crate::types::DataAsset::generate_asset_id(&owner, timestamp, &raw_data_hash);

        assert!(DataAssets::<T>::issue_certificate(
            RawOrigin::Signed(owner.clone()).into(),
            asset_id,
            holder.clone(),
            1u8,
            None,
        ).is_ok());

        // 生成正确的 certificate_id (与 RightToken::minimal 中的逻辑一致)
        let token_id = 0u32; // 第一个证书的 token_id 是 0
        let current_time = <pallet_timestamp::Pallet<T>>::get().saturated_into::<u64>();
        let certificate_id = crate::types::RightToken::<T::AccountId>::generate_certificate_id(
            &asset_id,
            current_time,
            &holder,
        );

        #[extrinsic_call]
        revoke_certificate(
            RawOrigin::Signed(owner.clone()),
            asset_id,
            certificate_id,
        );
    }

    #[benchmark]
    fn lock_asset() {
        // 设置
        let owner = create_funded_account::<T>("owner", 0);
        
        let name = b"Test Asset".to_vec();
        let description = b"Test Description".to_vec();
        let raw_data_hash = H256::repeat_byte(0x01);
        let data_size_bytes = 1024 * 1024;

        let collateral = T::BaseCollateral::get()
            .saturating_add(T::CollateralPerMB::get());
        T::Currency::make_free_balance_be(&owner, collateral * 10u32.into());

        assert!(DataAssets::<T>::register_asset(
            RawOrigin::Signed(owner.clone()).into(),
            name,
            description,
            raw_data_hash,
            data_size_bytes,
        ).is_ok());

        let timestamp = <pallet_timestamp::Pallet<T>>::get().saturated_into::<u64>();
        let asset_id = crate::types::DataAsset::generate_asset_id(&owner, timestamp, &raw_data_hash);

        #[extrinsic_call]
        lock_asset(RawOrigin::Signed(owner.clone()), asset_id);
    }

    #[benchmark]
    fn unlock_asset() {
        // 设置
        let owner = create_funded_account::<T>("owner", 0);
        
        let name = b"Test Asset".to_vec();
        let description = b"Test Description".to_vec();
        let raw_data_hash = H256::repeat_byte(0x01);
        let data_size_bytes = 1024 * 1024;

        let collateral = T::BaseCollateral::get()
            .saturating_add(T::CollateralPerMB::get());
        T::Currency::make_free_balance_be(&owner, collateral * 10u32.into());

        assert!(DataAssets::<T>::register_asset(
            RawOrigin::Signed(owner.clone()).into(),
            name,
            description,
            raw_data_hash,
            data_size_bytes,
        ).is_ok());

        let timestamp = <pallet_timestamp::Pallet<T>>::get().saturated_into::<u64>();
        let asset_id = crate::types::DataAsset::generate_asset_id(&owner, timestamp, &raw_data_hash);

        assert!(DataAssets::<T>::lock_asset(
            RawOrigin::Signed(owner.clone()).into(),
            asset_id,
        ).is_ok());

        #[extrinsic_call]
        unlock_asset(RawOrigin::Signed(owner.clone()), asset_id);
    }

    // ⚠️ 修复：使用正确的函数名 slash_asset_collateral
    #[benchmark]
    fn slash_collateral() {
        // 设置
        let owner = create_funded_account::<T>("owner", 0);
        
        let name = b"Test Asset".to_vec();
        let description = b"Test Description".to_vec();
        let raw_data_hash = H256::repeat_byte(0x01);
        let data_size_bytes = 1024 * 1024;

        let collateral = T::BaseCollateral::get()
            .saturating_add(T::CollateralPerMB::get());
        T::Currency::make_free_balance_be(&owner, collateral * 10u32.into());

        assert!(DataAssets::<T>::register_asset(
            RawOrigin::Signed(owner.clone()).into(),
            name,
            description,
            raw_data_hash,
            data_size_bytes,
        ).is_ok());

        let timestamp = <pallet_timestamp::Pallet<T>>::get().saturated_into::<u64>();
        let asset_id = crate::types::DataAsset::generate_asset_id(&owner, timestamp, &raw_data_hash);

        let slash_percentage = 50u8;

        #[extrinsic_call]
        slash_asset_collateral(RawOrigin::Root, asset_id, slash_percentage);
    }

    // ⚠️ 修复：使用正确的函数名 authorize_market
    #[benchmark]
    fn authorize_operator() {
        // 设置
        let owner = create_funded_account::<T>("owner", 0);
        let market = create_funded_account::<T>("market", 1);
        
        let name = b"Test Asset".to_vec();
        let description = b"Test Description".to_vec();
        let raw_data_hash = H256::repeat_byte(0x01);
        let data_size_bytes = 1024 * 1024;

        let collateral = T::BaseCollateral::get()
            .saturating_add(T::CollateralPerMB::get());
        T::Currency::make_free_balance_be(&owner, collateral * 10u32.into());

        assert!(DataAssets::<T>::register_asset(
            RawOrigin::Signed(owner.clone()).into(),
            name,
            description,
            raw_data_hash,
            data_size_bytes,
        ).is_ok());

        let timestamp = <pallet_timestamp::Pallet<T>>::get().saturated_into::<u64>();
        let asset_id = crate::types::DataAsset::generate_asset_id(&owner, timestamp, &raw_data_hash);

        #[extrinsic_call]
        authorize_market(
            RawOrigin::Signed(owner.clone()),
            asset_id,
            market,
        );
    }

    #[benchmark]
    fn revoke_authorization() {
        // 设置
        let owner = create_funded_account::<T>("owner", 0);
        let market = create_funded_account::<T>("market", 1);
        
        let name = b"Test Asset".to_vec();
        let description = b"Test Description".to_vec();
        let raw_data_hash = H256::repeat_byte(0x01);
        let data_size_bytes = 1024 * 1024;

        let collateral = T::BaseCollateral::get()
            .saturating_add(T::CollateralPerMB::get());
        T::Currency::make_free_balance_be(&owner, collateral * 10u32.into());

        assert!(DataAssets::<T>::register_asset(
            RawOrigin::Signed(owner.clone()).into(),
            name,
            description,
            raw_data_hash,
            data_size_bytes,
        ).is_ok());

        let timestamp = <pallet_timestamp::Pallet<T>>::get().saturated_into::<u64>();
        let asset_id = crate::types::DataAsset::generate_asset_id(&owner, timestamp, &raw_data_hash);

        assert!(DataAssets::<T>::authorize_market(
            RawOrigin::Signed(owner.clone()).into(),
            asset_id,
            market,
        ).is_ok());

        #[extrinsic_call]
        revoke_authorization(RawOrigin::Signed(owner.clone()), asset_id);
    }

    impl_benchmark_test_suite!(DataAssets, crate::tests::new_test_ext(), crate::tests::Test);
}