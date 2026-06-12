#![cfg_attr(not(feature = "std"), no_std, no_main)]

// `market_standard`被打包成了一个crate
use market_standard::{DataAssetsExtError, MarketStandard};

#[ink::contract(env = market_standard::CustomEnvironment)]
mod market_orderbook {
    use super::*;
    use ink::storage::Mapping;

    /// 订单信息
    #[derive(codec::Decode, codec::Encode, Debug, Clone)]
    #[cfg_attr(
        feature = "std",
        derive(scale_info::TypeInfo, ink::storage::traits::StorageLayout)
    )]
    pub struct Order {
        pub seller: AccountId,
        pub price: Balance,
        pub active: bool,
    }

    #[ink(storage)]
    pub struct MarketOrderbook {
        /// 资产ID -> 订单详情
        orders: Mapping<[u8; 32], Order>,
        /// (资产ID, 权证ID) -> 订单详情
        certificate_orders: Mapping<([u8; 32], [u8; 32]), Order>,
        /// 市场费率 (Basis Points)
        fee_ratio: u32,
        /// 管理员
        admin: AccountId,
    }

    /// 定义事件
    #[ink(event)]
    pub struct AssetListed {
        #[ink(topic)]
        asset_id: [u8; 32],
        seller: AccountId,
        price: Balance,
    }

    #[ink(event)]
    pub struct AssetSold {
        #[ink(topic)]
        asset_id: [u8; 32],
        buyer: AccountId,
        price: Balance,
    }

    #[ink(event)]
    pub struct AssetWithdrawn {
        #[ink(topic)]
        asset_id: [u8; 32],
        owner: AccountId,
    }

    #[ink(event)]
    pub struct CertificateListed {
        #[ink(topic)]
        asset_id: [u8; 32],
        #[ink(topic)]
        certificate_id: [u8; 32],
        seller: AccountId,
        price: Balance,
    }

    #[ink(event)]
    pub struct CertificateSold {
        #[ink(topic)]
        asset_id: [u8; 32],
        #[ink(topic)]
        certificate_id: [u8; 32],
        buyer: AccountId,
        price: Balance,
    }

    #[derive(Debug, PartialEq, Eq, codec::Encode, codec::Decode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
    #[allow(clippy::cast_possible_truncation)]
    pub enum Error {
        /// 调用链扩展失败
        ChainExtension(DataAssetsExtError),
        /// 资产已存在
        AssetAlreadyListed,
        /// 资产不存在
        AssetNotFound,
        /// 权限不足
        NotOwner,
        /// 资金不足
        InsufficientPayment,
        /// 转账失败
        TransferFailed,
    }

    // 将链扩展错误转换为合约错误
    impl From<DataAssetsExtError> for Error {
        fn from(e: DataAssetsExtError) -> Self {
            Error::ChainExtension(e)
        }
    }

    impl MarketOrderbook {
        #[ink(constructor)]
        pub fn new(fee_ratio: u32) -> Self {
            Self {
                orders: Mapping::default(),
                certificate_orders: Mapping::default(),
                fee_ratio,
                admin: Self::env().caller(),
            }
        }

        /// 【非标准接口】用户必须先调用此方法设置价格，然后调用 standard 的 asset_enter
        /// 或者在此方法内部调用 asset_enter 逻辑
        #[ink(message)]
        pub fn list_asset(&mut self, asset_id: [u8; 32], price: Balance) -> Result<(), Error> {
            if self.orders.contains(asset_id) {
                return Err(Error::AssetAlreadyListed);
            }

            // 注意：在实际场景中，Runtime 应该先确保资产已经转入合约账户 (escrow)
            // 只有资产在合约名下，合约才能在未来调用 transfer_asset 转出它

            let caller = self.env().caller();

            // 记录订单
            let order = Order {
                seller: caller,
                price,
                active: true,
            };
            self.orders.insert(asset_id, &order);

            // 触发标准中的进入逻辑（如果需要额外的状态变更是写在这里）
            self.asset_enter(asset_id);

            self.env().emit_event(AssetListed {
                asset_id,
                seller: caller,
                price,
            });

            Ok(())
        }

        /// 买家支付 native token，合约将 Asset 通过 Extension 转给买家
        #[ink(message, payable)]
        pub fn buy_asset(&mut self, asset_id: [u8; 32]) -> Result<(), Error> {
            let order = self.orders.get(asset_id).ok_or(Error::AssetNotFound)?;
            let caller = self.env().caller();
            let transferred_val = self.env().transferred_value();

            if transferred_val < order.price {
                return Err(Error::InsufficientPayment);
            }

            // 1. 先调用 Chain Extension 转移资产给买家，避免资产转移失败后仍给卖家付款。
            // 合约 (Self) -> 买家 (Caller)
            self.env().extension().transfer_asset(asset_id, caller)?;

            // 2. 给卖家转钱 (Native Token)
            if self.env().transfer(order.seller, order.price).is_err() {
                return Err(Error::TransferFailed);
            }

            // 3. 清理存储
            self.orders.remove(asset_id);

            // 4. 报告交易结果 (Standard Trait)
            // 现在生成一个假的 trade_id 用于演示
            let trade_id = [1u8; 32];
            self.report_trade_result(trade_id, true);

            self.env().emit_event(AssetSold {
                asset_id,
                buyer: caller,
                price: order.price,
            });

            Ok(())
        }

        /// 上架权证订单。权证应先由卖方转入市场合约账户托管。
        #[ink(message)]
        pub fn list_certificate(
            &mut self,
            asset_id: [u8; 32],
            certificate_id: [u8; 32],
            price: Balance,
        ) -> Result<(), Error> {
            let order_key = (asset_id, certificate_id);
            if self.certificate_orders.contains(order_key) {
                return Err(Error::AssetAlreadyListed);
            }

            let seller = self.env().caller();
            let order = Order {
                seller,
                price,
                active: true,
            };
            self.certificate_orders.insert(order_key, &order);

            self.env().emit_event(CertificateListed {
                asset_id,
                certificate_id,
                seller,
                price,
            });

            Ok(())
        }

        /// 买家支付 native token，合约将权证通过 Extension 转给买家。
        #[ink(message, payable)]
        pub fn buy_certificate(
            &mut self,
            asset_id: [u8; 32],
            certificate_id: [u8; 32],
        ) -> Result<(), Error> {
            let order_key = (asset_id, certificate_id);
            let order = self
                .certificate_orders
                .get(order_key)
                .ok_or(Error::AssetNotFound)?;
            let buyer = self.env().caller();
            let transferred_val = self.env().transferred_value();

            if transferred_val < order.price {
                return Err(Error::InsufficientPayment);
            }

            self.env()
                .extension()
                .transfer_certificate(asset_id, certificate_id, buyer)?;

            if self.env().transfer(order.seller, order.price).is_err() {
                return Err(Error::TransferFailed);
            }

            self.certificate_orders.remove(order_key);
            self.report_trade_result([2u8; 32], true);

            self.env().emit_event(CertificateSold {
                asset_id,
                certificate_id,
                buyer,
                price: order.price,
            });

            Ok(())
        }
    }

    /// 实现 MarketStandard Trait
    impl MarketStandard for MarketOrderbook {
        #[ink(message)]
        fn is_assetx_market(&self) -> bool {
            true
        }

        #[ink(message)]
        fn get_market_type(&self) -> u8 {
            0 // 0 代表 OrderBook
        }

        #[ink(message)]
        fn get_fee_ratio(&self) -> u32 {
            self.fee_ratio
        }

        #[ink(message)]
        fn check_admission(&self, _asset_id: [u8; 32]) -> bool {
            // 简单实现：允许所有资产
            true
        }

        #[ink(message)]
        fn can_list_asset(&self, asset_id: [u8; 32], _owner: AccountId) -> bool {
            // 如果订单表中不存在，则可以上架
            !self.orders.contains(asset_id)
        }

        #[ink(message)]
        fn asset_enter(&mut self, asset_id: [u8; 32]) {
            // 在 list_asset 中处理了主要逻辑
            // 这里可以做一些额外的统计或状态标记
            ink::env::debug_println!("Asset {:?} entered the market", asset_id);
        }

        #[ink(message)]
        fn asset_leave(&mut self, asset_id: [u8; 32]) {
            // 用户撤单逻辑
            let caller = self.env().caller();
            if let Some(order) = self.orders.get(asset_id) {
                if order.seller == caller {
                    // 1. 删除订单
                    self.orders.remove(asset_id);

                    // 2. 调用 Chain Extension 退还资产
                    // 合约 -> 卖家
                    let result = self.env().extension().transfer_asset(asset_id, caller);

                    if result.is_err() {
                        // 应该处理panic或回滚，这里打印日志
                        ink::env::debug_println!("Extension transfer failed!");
                        panic!("Failed to return asset via extension");
                    }

                    self.env().emit_event(AssetWithdrawn {
                        asset_id,
                        owner: caller,
                    });
                }
            }
        }

        #[ink(message)]
        fn report_trade_result(&mut self, trade_id: [u8; 32], success: bool) {
            ink::env::debug_println!("Trade {:?} finished. Success: {}", trade_id, success);
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use codec::Decode;
        use std::cell::RefCell;

        thread_local! {
            static RECORDED_ASSET_TRANSFER: RefCell<Option<(u16, [u8; 32], AccountId)>> =
                RefCell::new(None);
            static RECORDED_CERTIFICATE_TRANSFER: RefCell<Option<(u16, [u8; 32], [u8; 32], AccountId)>> =
                RefCell::new(None);
        }

        struct FailingTransferExtension;

        impl ink::env::test::ChainExtension for FailingTransferExtension {
            fn ext_id(&self) -> u16 {
                market_standard::DATA_ASSETS_EXT_ID as u16
            }

            fn call(&mut self, func_id: u16, _input: &[u8], _output: &mut Vec<u8>) -> u32 {
                assert!(
                    func_id == market_standard::TRANSFER_ASSET_FUNC_ID as u16
                        || func_id == market_standard::TRANSFER_CERT_FUNC_ID as u16
                );
                DataAssetsExtError::TransferFailed as u32
            }
        }

        struct RecordingTransferExtension;

        impl ink::env::test::ChainExtension for RecordingTransferExtension {
            fn ext_id(&self) -> u16 {
                market_standard::DATA_ASSETS_EXT_ID as u16
            }

            fn call(&mut self, func_id: u16, input: &[u8], _output: &mut Vec<u8>) -> u32 {
                let payload = Vec::<u8>::decode(&mut &input[..]).expect("encoded input payload");
                if func_id == market_standard::TRANSFER_ASSET_FUNC_ID as u16 {
                    let (asset_id, buyer) = <([u8; 32], AccountId)>::decode(&mut &payload[..])
                        .expect("valid asset transfer input");
                    RECORDED_ASSET_TRANSFER.with(|recorded| {
                        *recorded.borrow_mut() = Some((func_id, asset_id, buyer));
                    });
                } else if func_id == market_standard::TRANSFER_CERT_FUNC_ID as u16 {
                    let (asset_id, certificate_id, buyer) =
                        <([u8; 32], [u8; 32], AccountId)>::decode(&mut &payload[..])
                            .expect("valid certificate transfer input");
                    RECORDED_CERTIFICATE_TRANSFER.with(|recorded| {
                        *recorded.borrow_mut() = Some((func_id, asset_id, certificate_id, buyer));
                    });
                } else {
                    panic!("unexpected function id {func_id}");
                }
                0
            }
        }

        #[ink::test]
        fn failed_asset_transfer_keeps_payment_and_order_untouched() {
            let accounts = ink::env::test::default_accounts::<market_standard::CustomEnvironment>();
            ink::env::test::register_chain_extension(FailingTransferExtension);

            let contract = accounts.alice;
            let seller = accounts.bob;
            let buyer = accounts.charlie;
            let asset_id = [7u8; 32];
            let price = 100;

            ink::env::test::set_callee::<market_standard::CustomEnvironment>(contract);
            ink::env::test::set_caller::<market_standard::CustomEnvironment>(seller);

            let mut market = MarketOrderbook::new(0);
            assert_eq!(market.list_asset(asset_id, price), Ok(()));

            let seller_balance_before =
                ink::env::test::get_account_balance::<market_standard::CustomEnvironment>(seller)
                    .unwrap();

            ink::env::test::set_caller::<market_standard::CustomEnvironment>(buyer);
            ink::env::test::set_callee::<market_standard::CustomEnvironment>(contract);
            ink::env::test::transfer_in::<market_standard::CustomEnvironment>(price);

            assert_eq!(
                market.buy_asset(asset_id),
                Err(Error::ChainExtension(DataAssetsExtError::TransferFailed))
            );

            let seller_balance_after =
                ink::env::test::get_account_balance::<market_standard::CustomEnvironment>(seller)
                    .unwrap();
            assert_eq!(seller_balance_after, seller_balance_before);
            assert!(market.orders.contains(asset_id));
        }

        #[ink::test]
        fn buy_asset_calls_transfer_asset_extension_with_asset_and_buyer() {
            let accounts = ink::env::test::default_accounts::<market_standard::CustomEnvironment>();
            RECORDED_ASSET_TRANSFER.with(|recorded| *recorded.borrow_mut() = None);
            ink::env::test::register_chain_extension(RecordingTransferExtension);

            let contract = accounts.alice;
            let seller = accounts.bob;
            let buyer = accounts.charlie;
            let asset_id = [8u8; 32];
            let price = 100;

            ink::env::test::set_callee::<market_standard::CustomEnvironment>(contract);
            ink::env::test::set_caller::<market_standard::CustomEnvironment>(seller);

            let mut market = MarketOrderbook::new(0);
            assert_eq!(market.list_asset(asset_id, price), Ok(()));

            ink::env::test::set_caller::<market_standard::CustomEnvironment>(buyer);
            ink::env::test::set_callee::<market_standard::CustomEnvironment>(contract);
            ink::env::test::transfer_in::<market_standard::CustomEnvironment>(price);

            assert_eq!(market.buy_asset(asset_id), Ok(()));

            RECORDED_ASSET_TRANSFER.with(|recorded| {
                assert_eq!(
                    *recorded.borrow(),
                    Some((
                        market_standard::TRANSFER_ASSET_FUNC_ID as u16,
                        asset_id,
                        buyer
                    ))
                );
            });
            assert!(!market.orders.contains(asset_id));
        }

        #[ink::test]
        fn buy_certificate_calls_transfer_certificate_extension_with_certificate_and_buyer() {
            let accounts = ink::env::test::default_accounts::<market_standard::CustomEnvironment>();
            RECORDED_CERTIFICATE_TRANSFER.with(|recorded| *recorded.borrow_mut() = None);
            ink::env::test::register_chain_extension(RecordingTransferExtension);

            let contract = accounts.alice;
            let seller = accounts.bob;
            let buyer = accounts.charlie;
            let asset_id = [9u8; 32];
            let certificate_id = [10u8; 32];
            let price = 100;

            ink::env::test::set_callee::<market_standard::CustomEnvironment>(contract);
            ink::env::test::set_caller::<market_standard::CustomEnvironment>(seller);

            let mut market = MarketOrderbook::new(0);
            assert_eq!(
                market.list_certificate(asset_id, certificate_id, price),
                Ok(())
            );

            ink::env::test::set_caller::<market_standard::CustomEnvironment>(buyer);
            ink::env::test::set_callee::<market_standard::CustomEnvironment>(contract);
            ink::env::test::transfer_in::<market_standard::CustomEnvironment>(price);

            assert_eq!(market.buy_certificate(asset_id, certificate_id), Ok(()));

            RECORDED_CERTIFICATE_TRANSFER.with(|recorded| {
                assert_eq!(
                    *recorded.borrow(),
                    Some((
                        market_standard::TRANSFER_CERT_FUNC_ID as u16,
                        asset_id,
                        certificate_id,
                        buyer
                    ))
                );
            });
            assert!(!market
                .certificate_orders
                .contains((asset_id, certificate_id)));
        }

        #[ink::test]
        fn failed_certificate_transfer_keeps_payment_and_order_untouched() {
            let accounts = ink::env::test::default_accounts::<market_standard::CustomEnvironment>();
            ink::env::test::register_chain_extension(FailingTransferExtension);

            let contract = accounts.alice;
            let seller = accounts.bob;
            let buyer = accounts.charlie;
            let asset_id = [11u8; 32];
            let certificate_id = [12u8; 32];
            let price = 100;

            ink::env::test::set_callee::<market_standard::CustomEnvironment>(contract);
            ink::env::test::set_caller::<market_standard::CustomEnvironment>(seller);

            let mut market = MarketOrderbook::new(0);
            assert_eq!(
                market.list_certificate(asset_id, certificate_id, price),
                Ok(())
            );

            let seller_balance_before =
                ink::env::test::get_account_balance::<market_standard::CustomEnvironment>(seller)
                    .unwrap();

            ink::env::test::set_caller::<market_standard::CustomEnvironment>(buyer);
            ink::env::test::set_callee::<market_standard::CustomEnvironment>(contract);
            ink::env::test::transfer_in::<market_standard::CustomEnvironment>(price);

            assert_eq!(
                market.buy_certificate(asset_id, certificate_id),
                Err(Error::ChainExtension(DataAssetsExtError::TransferFailed))
            );

            let seller_balance_after =
                ink::env::test::get_account_balance::<market_standard::CustomEnvironment>(seller)
                    .unwrap();
            assert_eq!(seller_balance_after, seller_balance_before);
            assert!(market
                .certificate_orders
                .contains((asset_id, certificate_id)));
        }
    }
}
