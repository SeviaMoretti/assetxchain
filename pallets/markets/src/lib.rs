#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::pallet_prelude::*;
    use frame_system::pallet_prelude::*;
    use sp_runtime::traits::{
        AccountIdConversion, CheckedAdd, CheckedDiv, CheckedMul, CheckedSub, One, Zero
    };
    use frame_support::traits::{Currency, ExistenceRequirement, Get};
    use frame_support::PalletId;
    use sp_runtime::Perbill;
    use scale_info::TypeInfo;
    use sp_runtime::RuntimeDebug;
    use codec::{Encode, Decode, MaxEncodedLen, DecodeWithMemTracking};
    use pallet_shared_traits::DataAssetProvider;

    // AssetId[u8; 32]
    pub type AssetId = [u8; 32];
    // 简化 Balance 类型引用
    type BalanceOf<T> = <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

    // ===================================================================
    // 1. 数据结构定义 (Core Types)
    // ===================================================================

    /// 市场模型：创建时选定，决定了该市场支持哪种交易 Extrinsic
    #[derive(Encode, Decode, DecodeWithMemTracking, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
    pub enum MarketMechanism {
        /// 订单匹配模式：卖家挂单 (List)，买家购买 (Buy)
        OrderBook,
        /// 自动做市商模式：提供流动性 (AddLiquidity)，算法兑换 (Swap)
        AMM,
    }

    /// 准入规则：创建者自定义
    #[derive(Encode, Decode, DecodeWithMemTracking, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
    pub enum AccessRule {
        /// 无门槛
        Public,
        /// 质量门槛 (基于 DataAssets 模块的 quality_score)
        QualityGate {
            min_score: u8,          // 最低分数 (0-100)
            must_be_verified: bool, // 是否必须经过验证
        },
    }

    /// 市场基础信息
    #[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
    pub struct MarketInfo<AccountId> {
        pub creator: AccountId,
        pub mechanism: MarketMechanism, // 【核心】模型类型
        pub access_rule: AccessRule,    // 规则
        pub fee_ratio: u32,             // 手续费率 (Basis Points: 30 = 0.3%)
        pub active: bool,               // 激活状态
    }

    /// 挂单信息 (仅用于 OrderBook 市场)
    #[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
    pub struct ListingInfo<AccountId, Balance, MarketId> {
        pub market_id: MarketId, // 绑定所属市场，防止跨市场交易
        pub seller: AccountId,
        pub asset_id: AssetId,
        pub price: Balance,
    }

    /// 流动性池信息 (仅用于 AMM 市场)
    /// 假设交易的是 DataAsset 的“份额”或特定 AssetId 的数量
    /// 注意：为了演示 xy=k，这里假设 Asset 是可分割的或有数量概念
    #[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
    pub struct LiquidityPool<Balance> {
        pub token_reserve: Balance, // 资金储备
        pub asset_reserve: u128,    // 资产储备 (数量)
    }

    // ===================================================================
    // 2. Config Trait
    // ===================================================================
    #[pallet::config]
    pub trait Config: frame_system::Config + pallet_dataassets::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// 货币接口
        type Currency: Currency<Self::AccountId>;

        /// 市场模块的 Pallet ID，用于生成托管账户
        #[pallet::constant]
        type PalletId: Get<PalletId>;

        /// 市场 ID 类型
        type MarketId: Parameter + Member + Copy + Default + MaxEncodedLen + One + CheckedAdd + PartialEq;

        /// 挂单 ID 类型
        type ListingId: Parameter + Member + Copy + Default + MaxEncodedLen + One + CheckedAdd;
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    // ===================================================================
    // 3. Storage
    // ===================================================================
    
    // 计数器
    #[pallet::storage]
    pub type MarketCount<T: Config> = StorageValue<_, T::MarketId, ValueQuery>;
    #[pallet::storage]
    pub type ListingCount<T: Config> = StorageValue<_, T::ListingId, ValueQuery>;

    // 市场配置存储
    #[pallet::storage]
    #[pallet::getter(fn markets)]
    pub type Markets<T: Config> = StorageMap<_, Blake2_128Concat, T::MarketId, MarketInfo<T::AccountId>>;

    // [OrderBook 专用] 挂单存储
    #[pallet::storage]
    #[pallet::getter(fn listings)]
    pub type Listings<T: Config> = StorageMap<_, Blake2_128Concat, T::ListingId, ListingInfo<T::AccountId, BalanceOf<T>, T::MarketId>>;

    // [AMM 专用] 资金池存储
    #[pallet::storage]
    #[pallet::getter(fn pools)]
    pub type Pools<T: Config> = StorageMap<_, Blake2_128Concat, T::MarketId, LiquidityPool<BalanceOf<T>>>;

    // ===================================================================
    // 4. Events & Errors
    // ===================================================================
    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// 市场创建成功
        MarketCreated { market_id: T::MarketId, creator: T::AccountId, mechanism: MarketMechanism },
        /// [OrderBook] 挂单成功
        AssetListed { market_id: T::MarketId, listing_id: T::ListingId, seller: T::AccountId, asset_id: AssetId, price: BalanceOf<T> },
        /// [OrderBook] 购买成功
        AssetSold { market_id: T::MarketId, listing_id: T::ListingId, buyer: T::AccountId, price: BalanceOf<T> },
        /// [AMM] 流动性添加
        LiquidityAdded { market_id: T::MarketId, provider: T::AccountId, token_amount: BalanceOf<T>, asset_amount: u128 },
        /// [AMM] 兑换成功
        Swapped { market_id: T::MarketId, user: T::AccountId, amount_in: BalanceOf<T>, amount_out: u128 },
    }

    #[pallet::error]
    pub enum Error<T> {
        MarketNotFound,
        MarketNotActive,
        ListingNotFound,
        // 【核心错误】操作与市场模型不匹配
        WrongMarketMechanism,
        // 【核心错误】挂单不属于当前市场
        ListingNotInMarket,
        // 准入规则错误
        QualityTooLow,
        NotVerified,
        AssetNotFound,
        NotOwner,
        // 数学/余额错误
        MathOverflow,
        InsufficientLiquidity,
        InvalidAmount,
    }

    // ===================================================================
    // 5. Extrinsics (Calls)
    // ===================================================================
    #[pallet::call]
    impl<T: Config> Pallet<T> {
        
        /// 1. 创建市场
        /// 创建者在此决定：模型 (mechanism)、规则 (access_rule)、费率 (fee_ratio)
        #[pallet::call_index(0)]
        #[pallet::weight(10_000)]
        pub fn create_market(
            origin: OriginFor<T>,
            mechanism: MarketMechanism, 
            access_rule: AccessRule,
            fee_ratio: u32,
        ) -> DispatchResult {
            let creator = ensure_signed(origin)?;
            
            // 费率最大 100% (10000 basis points)
            ensure!(fee_ratio <= 10000, Error::<T>::MathOverflow);

            let market_id = MarketCount::<T>::get();
            let next_id = market_id.checked_add(&One::one()).ok_or(Error::<T>::MathOverflow)?;

            let info = MarketInfo {
                creator: creator.clone(),
                mechanism: mechanism.clone(),
                access_rule,
                fee_ratio,
                active: true,
            };

            Markets::<T>::insert(market_id, info);
            MarketCount::<T>::put(next_id);

            // 如果是 AMM，初始化空池子
            if mechanism == MarketMechanism::AMM {
                Pools::<T>::insert(market_id, LiquidityPool {
                    token_reserve: Zero::zero(),
                    asset_reserve: 0,
                });
            }

            Self::deposit_event(Event::MarketCreated { market_id, creator, mechanism });
            Ok(())
        }

        // -----------------------------------------------------------------
        // 场景 A: 订单匹配模式 (OrderBook)
        // -----------------------------------------------------------------

        /// 卖家挂单 (仅限 OrderBook 市场)
        #[pallet::call_index(1)]
        #[pallet::weight(10_000)]
        pub fn list_asset(
            origin: OriginFor<T>,
            market_id: T::MarketId,
            asset_id: AssetId,
            price: BalanceOf<T>,
        ) -> DispatchResult {
            let seller = ensure_signed(origin)?;
            let market = Markets::<T>::get(market_id).ok_or(Error::<T>::MarketNotFound)?;
            ensure!(market.active, Error::<T>::MarketNotActive);

            // 校验模型必须匹配
            ensure!(market.mechanism == MarketMechanism::OrderBook, Error::<T>::WrongMarketMechanism);

            // 校验资产所有权
            // let owner = pallet_dataassets::Pallet::<T>::get_asset_owner(asset_id)
            //     .ok_or(Error::<T>::AssetNotFound)?;
            let owner = DataAssetProvider::get_asset_owner(&asset_id)
                .map_err(|_| Error::<T>::AssetNotFound)?;
            ensure!(owner == seller, Error::<T>::NotOwner);

            // 校验准入规则 (Quality Check)
            Self::check_access_rule(&market, asset_id)?;

            // 生成 Listing ID
            let listing_id = ListingCount::<T>::get();
            let next_lid = listing_id.checked_add(&One::one()).ok_or(Error::<T>::MathOverflow)?;

            // 托管资产：Seller -> Market Account
            let market_account = Self::account_id();
            pallet_dataassets::Pallet::<T>::transfer_internal(&seller, &market_account, asset_id)?;

            // 存储挂单
            let listing = ListingInfo {
                market_id, // 强绑定
                seller: seller.clone(),
                asset_id,
                price,
            };
            Listings::<T>::insert(listing_id, listing);
            ListingCount::<T>::put(next_lid);

            Self::deposit_event(Event::AssetListed { market_id, listing_id, seller, asset_id, price });
            Ok(())
        }

        /// 买家购买 (仅限 OrderBook 市场)
        #[pallet::call_index(2)]
        #[pallet::weight(10_000)]
        pub fn buy_asset(
            origin: OriginFor<T>,
            market_id: T::MarketId,   // 买家指定市场
            listing_id: T::ListingId, // 买家指定单号
        ) -> DispatchResult {
            let buyer = ensure_signed(origin)?;
            let listing = Listings::<T>::get(listing_id).ok_or(Error::<T>::ListingNotFound)?;
            
            // 【校验 1】Listing 必须属于该 Market (隔离性)
            ensure!(listing.market_id == market_id, Error::<T>::ListingNotInMarket);

            let market = Markets::<T>::get(market_id).ok_or(Error::<T>::MarketNotFound)?;
            ensure!(market.active, Error::<T>::MarketNotActive);
            
            // 【校验 2】模型匹配 (双重保险)
            ensure!(market.mechanism == MarketMechanism::OrderBook, Error::<T>::WrongMarketMechanism);

            // 计算费用
            let (fee, seller_receive) = Self::calc_fee(listing.price, market.fee_ratio)?;

            // 资金划转：买家 -> 创建者 (Fee)
            if !fee.is_zero() {
                <T as Config>::Currency::transfer(&buyer, &market.creator, fee, ExistenceRequirement::KeepAlive)?;
            }
            // 资金划转：买家 -> 卖家 (货款)
            <T as Config>::Currency::transfer(&buyer, &listing.seller, seller_receive, ExistenceRequirement::KeepAlive)?;

            // 资产交割：Market Account -> Buyer
            let market_account = Self::account_id();
            pallet_dataassets::Pallet::<T>::transfer_internal(&market_account, &buyer, listing.asset_id)?;

            // 移除挂单
            Listings::<T>::remove(listing_id);

            Self::deposit_event(Event::AssetSold { market_id, listing_id, buyer, price: listing.price });
            Ok(())
        }

        // -----------------------------------------------------------------
        // 场景 B: 自动做市商模式 (AMM)
        // -----------------------------------------------------------------

        /// 添加流动性 (仅限 AMM 市场)
        /// 这里的 asset_amount 指的是数量，假设 DataAsset 支持数量或份额
        #[pallet::call_index(3)]
        #[pallet::weight(10_000)]
        pub fn add_liquidity(
            origin: OriginFor<T>,
            market_id: T::MarketId,
            token_amount: BalanceOf<T>,
            asset_amount: u128, 
        ) -> DispatchResult {
            let provider = ensure_signed(origin)?;
            let market = Markets::<T>::get(market_id).ok_or(Error::<T>::MarketNotFound)?;
            ensure!(market.active, Error::<T>::MarketNotActive);

            // 【校验】模型必须是 AMM
            ensure!(market.mechanism == MarketMechanism::AMM, Error::<T>::WrongMarketMechanism);

            // 转移 Token: Provider -> Market Account
            let market_account = Self::account_id();
            <T as Config>::Currency::transfer(&provider, &market_account, token_amount, ExistenceRequirement::KeepAlive)?;

            // 更新池子
            Pools::<T>::try_mutate(market_id, |maybe_pool| -> DispatchResult {
                let pool = maybe_pool.as_mut().ok_or(Error::<T>::MarketNotFound)?;
                pool.token_reserve = pool.token_reserve.checked_add(&token_amount).ok_or(Error::<T>::MathOverflow)?;
                pool.asset_reserve = pool.asset_reserve.checked_add(asset_amount).ok_or(Error::<T>::MathOverflow)?;
                Ok(())
            })?;

            // 注意：真实场景中，这里应该给 provider 发放 LP Token，这里简化了
            Self::deposit_event(Event::LiquidityAdded { market_id, provider, token_amount, asset_amount });
            Ok(())
        }

        /// AMM 兑换 (Buy Asset with Token)
        /// 这里简化为：Token -> Asset 单向买入
        #[pallet::call_index(4)]
        #[pallet::weight(10_000)]
        pub fn amm_swap_token_for_asset(
            origin: OriginFor<T>,
            market_id: T::MarketId,
            token_amount_in: BalanceOf<T>,
            min_asset_out: u128,
        ) -> DispatchResult {
            let buyer = ensure_signed(origin)?;
            let market = Markets::<T>::get(market_id).ok_or(Error::<T>::MarketNotFound)?;
            ensure!(market.active, Error::<T>::MarketNotActive);

            // 【校验】模型必须是 AMM
            ensure!(market.mechanism == MarketMechanism::AMM, Error::<T>::WrongMarketMechanism);

            // 1. 计算费用
            let (fee, amount_in_after_fee) = Self::calc_fee(token_amount_in, market.fee_ratio)?;

            // 2. 读取池子
            let mut pool = Pools::<T>::get(market_id).ok_or(Error::<T>::MarketNotFound)?;
            let reserve_token = pool.token_reserve;
            let reserve_asset = pool.asset_reserve;

            ensure!(reserve_asset > 0 && reserve_token > Zero::zero(), Error::<T>::InsufficientLiquidity);

            // 3. xy = k 公式计算 Output
            // (x + dx) * (y - dy) = x * y
            // dy = y - (x * y) / (x + dx) = y * dx / (x + dx)
            // asset_out = reserve_asset * amount_in / (reserve_token + amount_in)
            
            // 为了计算方便，将 Balance 转为 u128 (假设 Balance 就是 u128)
            let amount_in_u128: u128 = TryInto::<u128>::try_into(amount_in_after_fee).map_err(|_| Error::<T>::MathOverflow)?;
            let reserve_token_u128: u128 = TryInto::<u128>::try_into(reserve_token).map_err(|_| Error::<T>::MathOverflow)?;

            let numerator = amount_in_u128.checked_mul(reserve_asset).ok_or(Error::<T>::MathOverflow)?;
            let denominator = reserve_token_u128.checked_add(amount_in_u128).ok_or(Error::<T>::MathOverflow)?;
            let asset_out = numerator.checked_div(denominator).ok_or(Error::<T>::MathOverflow)?;

            ensure!(asset_out >= min_asset_out, Error::<T>::InsufficientLiquidity);

            // 4. 执行转账
            let market_account = Self::account_id();
            
            // Fee: Buyer -> Creator
            if !fee.is_zero() {
                <T as Config>::Currency::transfer(&buyer, &market.creator, fee, ExistenceRequirement::KeepAlive)?;
            }
            // Token: Buyer -> Market Account
            <T as Config>::Currency::transfer(&buyer, &market_account, amount_in_after_fee, ExistenceRequirement::KeepAlive)?;
            
            // Asset: Market -> Buyer (假设 DataAssets 支持份额/数量转账，如果只支持 ID，这里需要逻辑变更)
            // 这里仅仅作为逻辑演示
            // pallet_dataassets::Pallet::<T>::transfer_shares(&market_account, &buyer, asset_out)?;

            // 5. 更新池子
            pool.token_reserve = pool.token_reserve.checked_add(&amount_in_after_fee).ok_or(Error::<T>::MathOverflow)?;
            pool.asset_reserve = pool.asset_reserve.checked_sub(asset_out).ok_or(Error::<T>::MathOverflow)?;
            Pools::<T>::insert(market_id, pool);

            Self::deposit_event(Event::Swapped { market_id, user: buyer, amount_in: token_amount_in, amount_out: asset_out });
            Ok(())
        }
    }

    // ===================================================================
    // 6. 辅助函数
    // ===================================================================
    impl<T: Config> Pallet<T> {
        pub fn account_id() -> T::AccountId {
            T::PalletId::get().into_account_truncating()
        }

        /// 准入校验
        fn check_access_rule(market: &MarketInfo<T::AccountId>, asset_id: AssetId) -> DispatchResult {
            match market.access_rule {
                AccessRule::Public => Ok(()),
                AccessRule::QualityGate { min_score, must_be_verified } => {
                    let (score, is_verified) = pallet_dataassets::Pallet::<T>::get_asset_quality(asset_id)
                        .ok_or(Error::<T>::AssetNotFound)?;
                    
                    if must_be_verified {
                        ensure!(is_verified, Error::<T>::NotVerified);
                    }
                    ensure!(score >= min_score, Error::<T>::QualityTooLow);
                    Ok(())
                }
            }
        }

        /// 费用计算
        /// returns (fee_amount, remaining_amount)
        fn calc_fee(amount: BalanceOf<T>, ratio: u32) -> Result<(BalanceOf<T>, BalanceOf<T>), Error<T>> {
            // ratio is basis points (1/10000)
            let fee_part = Perbill::from_rational(ratio, 10000);
            let fee = fee_part * amount;
            let remaining = amount.checked_sub(&fee).ok_or(Error::<T>::MathOverflow)?;
            Ok((fee, remaining))
        }
    }
}