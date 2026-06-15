use crate as pallet_dataassets;
use crate::types::{DataAsset, RightToken, TradeAssetType};
use frame_support::{
    assert_noop, assert_ok, construct_runtime, derive_impl, parameter_types,
    traits::{ConstU128, ConstU32},
    weights::Weight,
};
use sp_core::H256;
use sp_runtime::BuildStorage;
use std::cell::RefCell;

type AccountId = u64;
type Balance = u128;
type Block = frame_system::mocking::MockBlock<Test>;

construct_runtime!(
    pub enum Test {
        System: frame_system,
        Balances: pallet_balances,
        Timestamp: pallet_timestamp,
        DataAssets: pallet_dataassets,
    }
);

pub struct TestWeightInfo;

impl crate::pallet::WeightInfo for TestWeightInfo {
    fn register_asset() -> Weight {
        Weight::zero()
    }
    fn register_asset_core() -> Weight {
        Weight::zero()
    }
    fn issue_certificate() -> Weight {
        Weight::zero()
    }
    fn transfer_certificate() -> Weight {
        Weight::zero()
    }
    fn transfer_asset() -> Weight {
        Weight::zero()
    }
    fn revoke_certificate() -> Weight {
        Weight::zero()
    }
    fn lock_asset() -> Weight {
        Weight::zero()
    }
    fn unlock_asset() -> Weight {
        Weight::zero()
    }
    fn slash_collateral() -> Weight {
        Weight::zero()
    }
    fn authorize_operator() -> Weight {
        Weight::zero()
    }
    fn revoke_authorization() -> Weight {
        Weight::zero()
    }
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
    type Block = Block;
    type AccountData = pallet_balances::AccountData<Balance>;
}

impl pallet_balances::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type Balance = Balance;
    type DustRemoval = ();
    type ExistentialDeposit = ConstU128<1>;
    type AccountStore = System;
    type WeightInfo = ();
    type MaxLocks = ConstU32<50>;
    type MaxReserves = ();
    type ReserveIdentifier = [u8; 8];
    type FreezeIdentifier = ();
    type MaxFreezes = ();
    type RuntimeHoldReason = ();
    type RuntimeFreezeReason = ();
    type DoneSlashHandler = ();
}

parameter_types! {
    pub const MinimumPeriod: u64 = 1;
    pub const BaseCollateral: Balance = 10;
    pub const CollateralPerMb: Balance = 1;
    pub const MaxCollateral: Balance = 100;
}

impl pallet_timestamp::Config for Test {
    type Moment = u64;
    type OnTimestampSet = ();
    type MinimumPeriod = MinimumPeriod;
    type WeightInfo = ();
}

thread_local! {
    static FIRST_CREATE_REWARDS: RefCell<Vec<([u8; 32], AccountId)>> = RefCell::new(Vec::new());
    static TRADE_MEASUREMENTS: RefCell<Vec<[u8; 32]>> = RefCell::new(Vec::new());
}

pub struct TradeRecorder;

impl pallet_shared_traits::IncentiveHandler<AccountId, [u8; 32], Balance> for TradeRecorder {
    fn distribute_first_create_reward(
        recipient: &AccountId,
        asset_id: &[u8; 32],
    ) -> Result<(), &'static str> {
        FIRST_CREATE_REWARDS.with(|rewards| rewards.borrow_mut().push((*asset_id, *recipient)));
        Ok(())
    }

    fn register_asset_trade(asset_id: &[u8; 32]) {
        TRADE_MEASUREMENTS.with(|measurements| measurements.borrow_mut().push(*asset_id));
    }

    fn distribute_liquidity_reward(
        _recipient: &AccountId,
        _order_amount: Balance,
    ) -> Result<(), &'static str> {
        Ok(())
    }

    fn distribute_proposal_reward(_recipient: &AccountId) -> Result<(), &'static str> {
        Ok(())
    }
}

impl pallet_dataassets::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type BaseCollateral = BaseCollateral;
    type CollateralPerMB = CollateralPerMb;
    type MaxCollateral = MaxCollateral;
    type MaxNameLength = ConstU32<256>;
    type MaxDescriptionLength = ConstU32<1024>;
    type IncentiveHandler = TradeRecorder;
    type WeightInfo = TestWeightInfo;
}

pub(crate) fn new_test_ext() -> sp_io::TestExternalities {
    let mut storage = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .unwrap();
    pallet_balances::GenesisConfig::<Test> {
        balances: vec![(1, 1_000), (2, 1_000), (3, 1_000), (4, 1_000)],
        ..Default::default()
    }
    .assimilate_storage(&mut storage)
    .unwrap();

    let mut ext = sp_io::TestExternalities::new(storage);
    ext.execute_with(|| {
        System::set_block_number(1);
        Timestamp::set_timestamp(42);
        FIRST_CREATE_REWARDS.with(|rewards| rewards.borrow_mut().clear());
        TRADE_MEASUREMENTS.with(|measurements| measurements.borrow_mut().clear());
    });
    ext
}

fn first_create_rewards() -> Vec<([u8; 32], AccountId)> {
    FIRST_CREATE_REWARDS.with(|rewards| rewards.borrow().clone())
}

fn trade_measurements() -> Vec<[u8; 32]> {
    TRADE_MEASUREMENTS.with(|measurements| measurements.borrow().clone())
}

fn register_test_asset(owner: AccountId) -> ([u8; 32], H256) {
    let raw_data_hash = H256::repeat_byte(0x11);
    assert_ok!(DataAssets::register_asset(
        RuntimeOrigin::signed(owner),
        b"asset".to_vec(),
        b"description".to_vec(),
        raw_data_hash,
        1024 * 1024,
    ));

    let asset_id = DataAsset::generate_asset_id(&owner, Timestamp::get(), &raw_data_hash);
    assert!(DataAssets::get_asset(&asset_id).is_some());
    (asset_id, raw_data_hash)
}

fn certificate_id(asset_id: &[u8; 32], issuer: AccountId, token_id: u32) -> [u8; 32] {
    RightToken::generate_certificate_id(asset_id, Timestamp::get(), &issuer, token_id)
}

fn trade_settled_event_count() -> usize {
    System::events()
        .iter()
        .filter(|record| {
            matches!(
                record.event,
                RuntimeEvent::DataAssets(pallet_dataassets::Event::TradeSettled { .. })
            )
        })
        .count()
}

#[test]
fn register_asset_core_creates_asset_without_collateral_or_incentive() {
    new_test_ext().execute_with(|| {
        let owner: AccountId = 1;
        let raw_data_hash = H256::repeat_byte(0x22);

        assert_ok!(DataAssets::register_asset_core(
            RuntimeOrigin::signed(owner),
            b"asset".to_vec(),
            b"description".to_vec(),
            raw_data_hash,
            1024 * 1024,
        ));

        let asset_id = DataAsset::generate_asset_id(&owner, Timestamp::get(), &raw_data_hash);
        let asset = DataAssets::get_asset(&asset_id).unwrap();
        assert_eq!(asset.core.owner, owner);
        assert_eq!(asset.core.token_id, 0);
        assert!(DataAssets::asset_collateral(asset_id).is_none());
        assert_eq!(Balances::reserved_balance(1), 0);
        assert!(first_create_rewards().is_empty());
        assert!(trade_measurements().is_empty());
    });
}

#[test]
fn issue_certificate_registers_certificate_trade_measurement() {
    new_test_ext().execute_with(|| {
        let (asset_id, _) = register_test_asset(1);

        assert_ok!(DataAssets::issue_certificate(
            RuntimeOrigin::signed(1),
            asset_id,
            2,
            1,
            None,
        ));

        assert_eq!(trade_measurements(), vec![asset_id]);
    });
}

#[test]
fn authorized_market_can_issue_certificate_to_buyer() {
    new_test_ext().execute_with(|| {
        let owner: AccountId = 1;
        let market: AccountId = 2;
        let buyer: AccountId = 3;
        let (asset_id, _) = register_test_asset(owner);

        assert_ok!(DataAssets::authorize_market(
            RuntimeOrigin::signed(owner),
            asset_id,
            market,
        ));

        let cert_id = DataAssets::issue_certificate_by_market_internal(
            &asset_id, &market, &owner, &buyer, 1, None,
        )
        .expect("authorized market issues certificate");

        let cert = DataAssets::get_certificate(&asset_id, &cert_id).unwrap();
        assert_eq!(cert.owner, buyer);
        assert_eq!(cert.issuer, owner);
        assert_eq!(cert.parent_asset_id, asset_id);
        assert_eq!(trade_measurements(), vec![asset_id]);
    });
}

#[test]
fn market_asset_settlement_records_trade_evidence() {
    new_test_ext().execute_with(|| {
        let owner: AccountId = 1;
        let market: AccountId = 3;
        let buyer: AccountId = 2;
        let price: Balance = 250;
        let (asset_id, _) = register_test_asset(owner);

        assert_ok!(DataAssets::authorize_market(
            RuntimeOrigin::signed(owner),
            asset_id,
            market,
        ));

        let trade_id =
            DataAssets::settle_asset_trade_by_market_internal(&asset_id, &market, &buyer, price)
                .expect("authorized market settles asset trade");

        let asset = DataAssets::get_asset(&asset_id).unwrap();
        assert_eq!(asset.core.owner, buyer);

        let settlement =
            DataAssets::trade_settlements(trade_id).expect("trade settlement is stored");
        assert_eq!(settlement.trade_id, trade_id);
        assert_eq!(settlement.market, market);
        assert_eq!(settlement.seller, owner);
        assert_eq!(settlement.buyer, buyer);
        assert_eq!(settlement.asset_id, asset_id);
        assert_eq!(settlement.certificate_id, [0u8; 32]);
        assert_eq!(settlement.asset_type, TradeAssetType::DataAsset);
        assert_eq!(settlement.price, price);
        assert_eq!(settlement.settled_at, System::block_number());

        System::assert_has_event(RuntimeEvent::DataAssets(
            pallet_dataassets::Event::AssetTransferred {
                asset_id,
                from: owner,
                to: buyer,
            },
        ));
        System::assert_has_event(RuntimeEvent::DataAssets(
            pallet_dataassets::Event::TradeSettled {
                trade_id,
                market,
                seller: owner,
                buyer,
                asset_id,
                certificate_id: [0u8; 32],
                asset_type: TradeAssetType::DataAsset,
                price,
                settled_at: System::block_number(),
            },
        ));
    });
}

#[test]
fn failed_market_asset_settlement_does_not_record_trade_evidence() {
    new_test_ext().execute_with(|| {
        let owner: AccountId = 1;
        let market: AccountId = 3;
        let buyer: AccountId = 2;
        let price: Balance = 250;
        let (asset_id, _) = register_test_asset(owner);

        assert_noop!(
            DataAssets::settle_asset_trade_by_market_internal(&asset_id, &market, &buyer, price),
            pallet_dataassets::Error::<Test>::NotAuthorized,
        );

        let asset = DataAssets::get_asset(&asset_id).unwrap();
        assert_eq!(asset.core.owner, owner);
        assert_eq!(trade_settled_event_count(), 0);
    });
}

#[test]
fn market_certificate_settlement_records_trade_evidence() {
    new_test_ext().execute_with(|| {
        let owner: AccountId = 1;
        let market: AccountId = 3;
        let buyer: AccountId = 2;
        let price: Balance = 75;
        let (asset_id, _) = register_test_asset(owner);

        assert_ok!(DataAssets::issue_certificate(
            RuntimeOrigin::signed(owner),
            asset_id,
            market,
            1,
            None,
        ));
        let cert_id = certificate_id(&asset_id, owner, 0);

        let trade_id = DataAssets::settle_certificate_trade_internal(
            &asset_id, &cert_id, &market, &buyer, price,
        )
        .expect("certificate holder market settles certificate trade");

        let cert = DataAssets::get_certificate(&asset_id, &cert_id).unwrap();
        assert_eq!(cert.owner, buyer);

        let settlement =
            DataAssets::trade_settlements(trade_id).expect("trade settlement is stored");
        assert_eq!(settlement.trade_id, trade_id);
        assert_eq!(settlement.market, market);
        assert_eq!(settlement.seller, market);
        assert_eq!(settlement.buyer, buyer);
        assert_eq!(settlement.asset_id, asset_id);
        assert_eq!(settlement.certificate_id, cert_id);
        assert_eq!(settlement.asset_type, TradeAssetType::Certificate);
        assert_eq!(settlement.price, price);
        assert_eq!(settlement.settled_at, System::block_number());

        System::assert_has_event(RuntimeEvent::DataAssets(
            pallet_dataassets::Event::CertificateTransferred {
                asset_id,
                certificate_id: cert_id,
                from: market,
                to: buyer,
            },
        ));
        System::assert_has_event(RuntimeEvent::DataAssets(
            pallet_dataassets::Event::TradeSettled {
                trade_id,
                market,
                seller: market,
                buyer,
                asset_id,
                certificate_id: cert_id,
                asset_type: TradeAssetType::Certificate,
                price,
                settled_at: System::block_number(),
            },
        ));
    });
}

#[test]
fn failed_market_certificate_settlement_does_not_record_trade_evidence() {
    new_test_ext().execute_with(|| {
        let owner: AccountId = 1;
        let market: AccountId = 3;
        let buyer: AccountId = 2;
        let price: Balance = 75;
        let (asset_id, _) = register_test_asset(owner);
        let missing_cert_id = [9u8; 32];

        assert_noop!(
            DataAssets::settle_certificate_trade_internal(
                &asset_id,
                &missing_cert_id,
                &market,
                &buyer,
                price,
            ),
            pallet_dataassets::Error::<Test>::CertificateNotFound,
        );

        assert_eq!(trade_settled_event_count(), 0);
    });
}

#[test]
fn unauthorized_market_cannot_issue_certificate() {
    new_test_ext().execute_with(|| {
        let owner: AccountId = 1;
        let market: AccountId = 2;
        let buyer: AccountId = 3;
        let (asset_id, _) = register_test_asset(owner);

        assert_noop!(
            DataAssets::issue_certificate_by_market_internal(
                &asset_id, &market, &owner, &buyer, 1, None,
            ),
            pallet_dataassets::Error::<Test>::NotAuthorized,
        );

        let cert_id = certificate_id(&asset_id, owner, 0);
        assert!(DataAssets::get_certificate(&asset_id, &cert_id).is_none());
        assert!(trade_measurements().is_empty());
    });
}

#[test]
fn authorized_market_cannot_issue_certificate_for_non_owner_seller() {
    new_test_ext().execute_with(|| {
        let owner: AccountId = 1;
        let market: AccountId = 2;
        let buyer: AccountId = 3;
        let fake_seller: AccountId = 4;
        let (asset_id, _) = register_test_asset(owner);

        assert_ok!(DataAssets::authorize_market(
            RuntimeOrigin::signed(owner),
            asset_id,
            market,
        ));

        assert_noop!(
            DataAssets::issue_certificate_by_market_internal(
                &asset_id,
                &market,
                &fake_seller,
                &buyer,
                1,
                None,
            ),
            pallet_dataassets::Error::<Test>::NotOwner,
        );

        let cert_id = certificate_id(&asset_id, owner, 0);
        assert!(DataAssets::get_certificate(&asset_id, &cert_id).is_none());
        assert!(trade_measurements().is_empty());
    });
}

#[test]
fn transfer_certificate_updates_owner_nonce_and_registers_trade_measurement() {
    new_test_ext().execute_with(|| {
        let (asset_id, _) = register_test_asset(1);

        assert_ok!(DataAssets::issue_certificate(
            RuntimeOrigin::signed(1),
            asset_id,
            2,
            1,
            None,
        ));

        let cert_id = certificate_id(&asset_id, 1, 0);
        let cert = DataAssets::get_certificate(&asset_id, &cert_id).unwrap();
        assert_eq!(cert.owner, 2);
        assert_eq!(cert.nonce, 0);

        assert_ok!(DataAssets::transfer_certificate(
            RuntimeOrigin::signed(2),
            asset_id,
            cert_id,
            3,
        ));

        let cert = DataAssets::get_certificate(&asset_id, &cert_id).unwrap();
        assert_eq!(cert.owner, 3);
        assert_eq!(cert.nonce, 1);
        assert_eq!(trade_measurements(), vec![asset_id, asset_id]);
    });
}

#[test]
fn transfer_certificate_rejects_non_owner_without_measurement() {
    new_test_ext().execute_with(|| {
        let (asset_id, _) = register_test_asset(1);

        assert_ok!(DataAssets::issue_certificate(
            RuntimeOrigin::signed(1),
            asset_id,
            2,
            1,
            None,
        ));

        let cert_id = certificate_id(&asset_id, 1, 0);

        assert_noop!(
            DataAssets::transfer_certificate(RuntimeOrigin::signed(3), asset_id, cert_id, 4),
            pallet_dataassets::Error::<Test>::NotOwner,
        );

        let cert = DataAssets::get_certificate(&asset_id, &cert_id).unwrap();
        assert_eq!(cert.owner, 2);
        assert_eq!(cert.nonce, 0);
        assert_eq!(trade_measurements(), vec![asset_id]);
    });
}

#[test]
fn transfer_certificate_rejects_expired_certificate_without_state_change() {
    new_test_ext().execute_with(|| {
        let (asset_id, _) = register_test_asset(1);

        assert_ok!(DataAssets::issue_certificate(
            RuntimeOrigin::signed(1),
            asset_id,
            2,
            1,
            Some(Timestamp::get() - 1),
        ));

        let cert_id = certificate_id(&asset_id, 1, 0);

        assert_noop!(
            DataAssets::transfer_certificate(RuntimeOrigin::signed(2), asset_id, cert_id, 3),
            pallet_dataassets::Error::<Test>::CertificateNotActive,
        );

        let cert = DataAssets::get_certificate(&asset_id, &cert_id).unwrap();
        assert_eq!(cert.owner, 2);
        assert_eq!(cert.nonce, 0);
        assert_eq!(trade_measurements(), vec![asset_id]);
    });
}

#[test]
fn transfer_certificate_rejects_revoked_certificate_without_measurement() {
    new_test_ext().execute_with(|| {
        let (asset_id, _) = register_test_asset(1);

        assert_ok!(DataAssets::issue_certificate(
            RuntimeOrigin::signed(1),
            asset_id,
            2,
            1,
            None,
        ));

        let cert_id = certificate_id(&asset_id, 1, 0);
        assert_ok!(DataAssets::revoke_certificate(
            RuntimeOrigin::signed(1),
            asset_id,
            cert_id,
        ));

        assert_noop!(
            DataAssets::transfer_certificate(RuntimeOrigin::signed(2), asset_id, cert_id, 3),
            pallet_dataassets::Error::<Test>::CertificateNotFound,
        );
        assert!(DataAssets::get_certificate(&asset_id, &cert_id).is_none());
        assert_eq!(trade_measurements(), vec![asset_id]);
    });
}
