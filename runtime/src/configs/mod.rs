// This is free and unencumbered software released into the public domain.
//
// Anyone is free to copy, modify, publish, use, compile, sell, or
// distribute this software, either in source code form or as a compiled
// binary, for any purpose, commercial or non-commercial, and by any
// means.
//
// In jurisdictions that recognize copyright laws, the author or authors
// of this software dedicate any and all copyright interest in the
// software to the public domain. We make this dedication for the benefit
// of the public at large and to the detriment of our heirs and
// successors. We intend this dedication to be an overt act of
// relinquishment in perpetuity of all present and future rights to this
// software under copyright law.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
// EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
// IN NO EVENT SHALL THE AUTHORS BE LIABLE FOR ANY CLAIM, DAMAGES OR
// OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE,
// ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
// OTHER DEALINGS IN THE SOFTWARE.
//
// For more information, please refer to <http://unlicense.org>

// Substrate and Polkadot dependencies
use frame_support::{
	derive_impl, parameter_types,
	traits::{ConstU128, ConstU32, ConstU64, ConstU8, VariantCountOf, WithdrawReasons, Get},
	weights::{
		constants::{RocksDbWeight, WEIGHT_REF_TIME_PER_SECOND},
		IdentityFee, Weight,
	},
};
use frame_system::pallet::Pallet as SystemPallet;
use frame_system::limits::{BlockLength, BlockWeights};
use pallet_transaction_payment::{ConstFeeMultiplier, FungibleAdapter, Multiplier};
use sp_runtime::traits::OpaqueKeys;
use sp_runtime::{traits::One, Perbill};
use sp_version::RuntimeVersion;

// Local module imports
use super::{
	AccountId, Balance, Balances, Block, BlockNumber, Hash, Nonce, PalletInfo, Runtime,
	RuntimeCall, RuntimeEvent, RuntimeFreezeReason, RuntimeHoldReason, RuntimeOrigin, RuntimeTask,
	System, EXISTENTIAL_DEPOSIT, SLOT_DURATION, VERSION, DAYS, HOURS, MILLI_SECS_PER_BLOCK,
	Babe, SessionKeys, Vesting,
};
use crate::UNIT;

const NORMAL_DISPATCH_RATIO: Perbill = Perbill::from_percent(75);

parameter_types! {
	pub const BlockHashCount: BlockNumber = 2400;
	pub const Version: RuntimeVersion = VERSION;

	/// We allow for 2 seconds of compute with a 6 second average block time.
	pub RuntimeBlockWeights: BlockWeights = BlockWeights::with_sensible_defaults(
		Weight::from_parts(2u64 * WEIGHT_REF_TIME_PER_SECOND, u64::MAX),
		NORMAL_DISPATCH_RATIO,
	);
	pub RuntimeBlockLength: BlockLength = BlockLength::max_with_normal_ratio(5 * 1024 * 1024, NORMAL_DISPATCH_RATIO);
	pub const SS58Prefix: u8 = 42;
}

/// The default types are being injected by [`derive_impl`](`frame_support::derive_impl`) from
/// [`SoloChainDefaultConfig`](`struct@frame_system::config_preludes::SolochainDefaultConfig`),
/// but overridden as needed.
#[derive_impl(frame_system::config_preludes::SolochainDefaultConfig)]
impl frame_system::Config for Runtime {
	/// The block type for the runtime.
	type Block = Block;
	/// Block & extrinsics weights: base values and limits.
	type BlockWeights = RuntimeBlockWeights;
	/// The maximum length of a block (in bytes).
	type BlockLength = RuntimeBlockLength;
	/// The identifier used to distinguish between accounts.
	type AccountId = AccountId;
	/// The type for storing how many extrinsics an account has signed.
	type Nonce = Nonce;
	/// The type for hashing blocks and tries.
	type Hash = Hash;
	/// Maximum number of block number to block hash mappings to keep (oldest pruned first).
	type BlockHashCount = BlockHashCount;
	/// The weight of database operations that the runtime can invoke.
	type DbWeight = RocksDbWeight;
	/// Version of the runtime.
	type Version = Version;
	/// The data to be stored in an account.
	type AccountData = pallet_balances::AccountData<Balance>;
	/// This is used as an identifier of the chain. 42 is the generic substrate prefix.
	type SS58Prefix = SS58Prefix;
	type MaxConsumers = frame_support::traits::ConstU32<16>;
}

// BABE参数
parameter_types! {
    // Epoch 持续时间（slot 数量）
    // 2400 个 slot × 6 秒 = 4 小时一个 epoch
    pub const EpochDuration: u64 = 2400;
    
    // 期望的区块时间（毫秒）
    pub const ExpectedBlockTime: u64 = MILLI_SECS_PER_BLOCK;
    
    // 报告延迟
    pub const ReportLongevity: u64 = 
        ((DAYS as u64) * 7) as u64;
}

// Session
parameter_types! {
    pub const Period: u32 = 6 * HOURS; // 一个 session 6 小时
    pub const Offset: u32 = 0;
}

impl pallet_babe::Config for Runtime {
    type EpochDuration = EpochDuration;
    type ExpectedBlockTime = ExpectedBlockTime;
    type EpochChangeTrigger = pallet_babe::SameAuthoritiesForever;
    type DisabledValidators = (); //pallet_session::Pallet<Runtime>;
    type WeightInfo = ();
    type MaxAuthorities = frame_support::traits::ConstU32<32>;
    type MaxNominators = frame_support::traits::ConstU32<0>; // 暂时不使用 nominator
    type KeyOwnerProof = sp_core::Void; // 简化
    type EquivocationReportSystem = (); // 简化
}

pub struct ValidatorIdOf;
impl sp_runtime::traits::Convert<AccountId, Option<AccountId>> for ValidatorIdOf {
    fn convert(account: AccountId) -> Option<AccountId> {
        Some(account)
    }
}

impl pallet_session::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type ValidatorId = AccountId;
    type ValidatorIdOf = ValidatorIdOf;
    type ShouldEndSession = pallet_session::PeriodicSessions<Period, Offset>;
    type NextSessionRotation = pallet_session::PeriodicSessions<Period, Offset>;
    type SessionManager = (); // 简化
    type SessionHandler = <SessionKeys as OpaqueKeys>::KeyTypeIdProviders;
    type Keys = SessionKeys;
    type WeightInfo = ();
	type DisablingStrategy = ();
}

impl pallet_authorship::Config for Runtime {
    type FindAuthor = pallet_session::FindAccountFromAuthorIndex<Self, Babe>;
    type EventHandler = (); // 之后添加区块奖励处理
}

// impl pallet_aura::Config for Runtime {
// 	type AuthorityId = AuraId;
// 	type DisabledValidators = ();
// 	type MaxAuthorities = ConstU32<32>;
// 	type AllowMultipleBlocksPerSlot = ConstBool<false>;
// 	type SlotDuration = pallet_aura::MinimumPeriodTimesTwo<Runtime>;
// }

impl pallet_grandpa::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;

	type WeightInfo = ();
	type MaxAuthorities = ConstU32<32>;
	type MaxNominators = ConstU32<0>;
	type MaxSetIdSessionEntries = ConstU64<0>;

	type KeyOwnerProof = sp_core::Void;
	type EquivocationReportSystem = ();
}

impl pallet_timestamp::Config for Runtime {
	/// A timestamp: milliseconds since the unix epoch.
	type Moment = u64;
	type OnTimestampSet = Babe;
	type MinimumPeriod = ConstU64<{ SLOT_DURATION / 2 }>;
	type WeightInfo = ();
}

impl pallet_balances::Config for Runtime {
	type MaxLocks = ConstU32<50>; // 支持质押时的锁仓数量
	type MaxReserves = (); // 支持多池储备，目前未启用
	type ReserveIdentifier = [u8; 8]; // 区分不同的储备池的标识符（例如："DATA"）
	/// The type for recording an account's balance.
	type Balance = Balance;
	/// The ubiquitous event type.
	type RuntimeEvent = RuntimeEvent;
	type DustRemoval = (); // 粉尘清理，关闭
	type ExistentialDeposit = ConstU128<EXISTENTIAL_DEPOSIT>;
	type AccountStore = System;
	type WeightInfo = pallet_balances::weights::SubstrateWeight<Runtime>;
	type FreezeIdentifier = RuntimeFreezeReason;
	type MaxFreezes = VariantCountOf<RuntimeFreezeReason>;
	type RuntimeHoldReason = RuntimeHoldReason;
	type RuntimeFreezeReason = RuntimeFreezeReason;
	type DoneSlashHandler = ();
}

parameter_types! {
	pub FeeMultiplier: Multiplier = Multiplier::one();
}

impl pallet_transaction_payment::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type OnChargeTransaction = FungibleAdapter<Balances, ()>;
	type OperationalFeeMultiplier = ConstU8<5>;
	type WeightToFee = IdentityFee<Balance>;
	type LengthToFee = IdentityFee<Balance>;
	type FeeMultiplierUpdate = ConstFeeMultiplier<FeeMultiplier>;
	type WeightInfo = pallet_transaction_payment::weights::SubstrateWeight<Runtime>;
}

impl pallet_sudo::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type WeightInfo = pallet_sudo::weights::SubstrateWeight<Runtime>;
}

/// Configure the pallet-template in pallets/template.
impl pallet_template::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = pallet_template::weights::SubstrateWeight<Runtime>;
}

parameter_types! {
    /// Base collateral amount: 2000 DATA
    /// This is the minimum collateral required for any asset
    pub const BaseCollateral: Balance = 2_000 * UNIT;
    
    /// Collateral per MB: 100 DATA/MB
    /// Additional collateral based on data size
    pub const CollateralPerMB: Balance = 100 * UNIT;
    
    /// Maximum collateral cap: 75000 DATA
    /// Upper limit to prevent excessive collateral requirements
    pub const MaxCollateral: Balance = 75_000 * UNIT;

	/// Maximum number of release phases for collateral
	pub const MaxReleasePhases: u32 = 5;
}

impl pallet_dataassets::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    
    /// Use Balances pallet for collateral management
    type Currency = Balances;
    
    /// Collateral configuration
    type BaseCollateral = BaseCollateral;
    type CollateralPerMB = CollateralPerMB;
    type MaxCollateral = MaxCollateral;
    
    /// Asset metadata constraints
    type MaxNameLength = ConstU32<256>;
    type MaxDescriptionLength = ConstU32<1024>;
}

// 添加参数配置
parameter_types! {
    // 5年线性释放 (按区块计算: 5年 * 365天 * 24小时 * 60分钟 * 60秒 / 18秒每区块)
    pub const FoundationVestingPeriod: BlockNumber = 5 * 365 * 24 * 60 * 60 / (MILLI_SECS_PER_BLOCK / 1000) as BlockNumber;
    pub const MinVestedTransfer: Balance = 1 * UNIT;
	pub const MaxVestingSchedules: u32 = 20;
	pub const AllowedWithdrawReasons: WithdrawReasons = WithdrawReasons::from_bits(
		WithdrawReasons::TRANSFER.bits() | WithdrawReasons::TRANSACTION_PAYMENT.bits()
	).expect("Valid bits");
}

// 实现vesting配置
impl pallet_vesting::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type BlockNumberToBalance = sp_runtime::traits::ConvertInto;
    type MinVestedTransfer = MinVestedTransfer;
    type WeightInfo = pallet_vesting::weights::SubstrateWeight<Runtime>;
    // 使用预定义的常量代替直接在类型参数中计算
    type UnvestedFundsAllowedWithdrawReasons = AllowedWithdrawReasons;
    type BlockNumberProvider = SystemPallet<Runtime>;
    // 之前修复的 MAX_VESTING_SCHEDULES 常量
    const MAX_VESTING_SCHEDULES: u32 = MaxVestingSchedules::get();
}


impl crate::custom_header::AssetsStateRootProvider<sp_runtime::traits::BlakeTwo256> for Runtime {
    fn compute_assets_state_root() -> sp_core::H256 {
        pallet_dataassets::Pallet::<Runtime>::compute_asset_root()
    }
}