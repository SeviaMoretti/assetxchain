#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode};
use ink::env::Environment;
use ink::primitives::AccountId;
use scale_info::TypeInfo;

// 链扩展ID（u32类型）
pub const DATA_ASSETS_EXT_ID: u32 = 1;
pub const TRANSFER_ASSET_FUNC_ID: u32 = 1; // 方法ID
pub const TRANSFER_CERT_FUNC_ID: u32 = 2; // 权证转移方法ID
pub const ISSUE_CERT_FUNC_ID: u32 = 3; // 权证发行方法ID
                                       // 链扩展错误码
#[derive(Debug, Copy, Clone, PartialEq, Eq, Encode, Decode, TypeInfo)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub enum DataAssetsExtError {
    TransferFailed = 1,
    AssetNotFound = 2,
    PermissionDenied = 3,
    CertificateNotFound = 4,
    CertificateNotActive = 5,
}

// 为 DataAssetsExtError 实现 FromStatusCode trait
impl ink::env::chain_extension::FromStatusCode for DataAssetsExtError {
    fn from_status_code(status_code: u32) -> Result<(), Self> {
        match status_code {
            0 => Ok(()),
            1 => Err(Self::TransferFailed),
            2 => Err(Self::AssetNotFound),
            3 => Err(Self::PermissionDenied),
            4 => Err(Self::CertificateNotFound),
            5 => Err(Self::CertificateNotActive),
            _ => panic!("unknown status code"),
        }
    }
}

// 为 DataAssetsExtError 实现 From<scale::Error> trait
impl From<scale_info::scale::Error> for DataAssetsExtError {
    fn from(_: scale_info::scale::Error) -> Self {
        // 这里可以根据需要将编解码错误映射为特定的错误类型
        DataAssetsExtError::TransferFailed
    }
}

#[ink::chain_extension(extension = 1)]
pub trait DataAssetsExt {
    type ErrorCode = DataAssetsExtError;

    /// 转移资产
    /// 对应 Runtime 中的 func_id = 1
    #[ink(function = 1)]
    fn transfer_asset(asset_id: [u8; 32], to: AccountId) -> Result<(), DataAssetsExtError>;

    /// 转移权证
    /// 对应 Runtime 中的 func_id = 2
    #[ink(function = 2)]
    fn transfer_certificate(
        asset_id: [u8; 32],
        certificate_id: [u8; 32],
        to: AccountId,
    ) -> Result<(), DataAssetsExtError>;

    /// 发行权证
    /// 对应 Runtime 中的 func_id = 3
    #[ink(function = 3)]
    fn issue_certificate(
        asset_id: [u8; 32],
        issuer: AccountId,
        holder: AccountId,
        right_type: u8,
        valid_until: Option<u64>,
    ) -> Result<(), DataAssetsExtError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
pub enum CustomEnvironment {}

impl Environment for CustomEnvironment {
    const MAX_EVENT_TOPICS: usize = <ink::env::DefaultEnvironment as Environment>::MAX_EVENT_TOPICS;
    type AccountId = <ink::env::DefaultEnvironment as Environment>::AccountId;
    type Balance = <ink::env::DefaultEnvironment as Environment>::Balance;
    type Hash = <ink::env::DefaultEnvironment as Environment>::Hash;
    type BlockNumber = <ink::env::DefaultEnvironment as Environment>::BlockNumber;
    type Timestamp = <ink::env::DefaultEnvironment as Environment>::Timestamp;

    type ChainExtension = DataAssetsExt;
}

/// 市场标准接口
/// 所有想要接入AssetxChain的市场必须实现这些方法
#[ink::trait_definition]
pub trait MarketStandard {
    /// 必须返回 true，证明合约“声称”自己符合标准
    #[ink(message)]
    fn is_assetx_market(&self) -> bool;

    /// 获取市场类型 (0:OrderBook,1:Auction,2:Swap等等)
    #[ink(message)]
    fn get_market_type(&self) -> u8;

    /// 获取当前交易费率(Basis Points,30 = 0.3%)
    /// 用户和前端查询费率，防止隐形收费
    #[ink(message)]
    fn get_fee_ratio(&self) -> u32;

    /// 检查资产准入
    #[ink(message)]
    fn check_admission(&self, asset_id: [u8; 32]) -> bool;

    /// 【准入】：检查某个资产ID是否允许在此市场交易
    #[ink(message)]
    fn can_list_asset(&self, asset_id: [u8; 32], owner: AccountId) -> bool;

    /// 投入市场,用户需要先调用can_list_asset检查资产是否允许交易,
    #[ink(message)]
    fn asset_enter(&mut self, asset_id: [u8; 32]);

    /// 退出市场,用户可以调用此方法退出市场,资产将返回用户
    #[ink(message)]
    fn asset_leave(&mut self, asset_id: [u8; 32]);

    /// 报告交易结果,用户需要调用此方法报告交易结果,
    /// 市场合约会根据交易结果更新资产状态
    #[ink(message)]
    fn report_trade_result(&mut self, trade_id: [u8; 32], success: bool);
    // 注意：交易功能 (buy, list) 通常不放在标准 Trait 里强制要求同名，
    // 因为不同模式参数不同（拍卖需要起拍价、时间；一口价只需要价格）。
    // 这些通过前端适配或 ABI 解析来处理。
}
#[cfg(test)]
mod tests {
    use super::*;
    use codec::Encode;
    use ink::env::chain_extension::FromStatusCode;
    use ink::env::test;
    use std::cell::RefCell;

    thread_local! {
        static CERTIFICATE_TRANSFER_CALL: RefCell<Option<(u16, Vec<u8>)>> = RefCell::new(None);
    }

    struct RecordingCertificateTransferExtension;

    impl test::ChainExtension for RecordingCertificateTransferExtension {
        fn ext_id(&self) -> u16 {
            DATA_ASSETS_EXT_ID as u16
        }

        fn call(&mut self, func_id: u16, input: &[u8], _output: &mut Vec<u8>) -> u32 {
            CERTIFICATE_TRANSFER_CALL.with(|call| {
                *call.borrow_mut() = Some((func_id, input.to_vec()));
            });
            0
        }
    }

    #[test]
    fn status_code_4_maps_to_certificate_not_found() {
        assert_eq!(
            DataAssetsExtError::from_status_code(4),
            Err(DataAssetsExtError::CertificateNotFound)
        );
    }

    #[test]
    fn status_code_5_maps_to_certificate_not_active() {
        assert_eq!(
            DataAssetsExtError::from_status_code(5),
            Err(DataAssetsExtError::CertificateNotActive)
        );
    }

    #[ink::test]
    fn transfer_certificate_extension_uses_function_id_2_and_expected_input() {
        CERTIFICATE_TRANSFER_CALL.with(|call| *call.borrow_mut() = None);
        test::register_chain_extension(RecordingCertificateTransferExtension);

        let accounts = test::default_accounts::<CustomEnvironment>();
        let asset_id = [7u8; 32];
        let certificate_id = [8u8; 32];

        ink::EnvAccess::<CustomEnvironment>::default()
            .extension()
            .transfer_certificate(asset_id, certificate_id, accounts.bob)
            .expect("certificate transfer succeeds");

        let expected_payload = (asset_id, certificate_id, accounts.bob).encode();
        let expected_input = expected_payload.encode();
        CERTIFICATE_TRANSFER_CALL.with(|call| {
            assert_eq!(
                *call.borrow(),
                Some((TRANSFER_CERT_FUNC_ID as u16, expected_input))
            );
        });
    }

    #[ink::test]
    fn issue_certificate_extension_uses_function_id_3_and_expected_input() {
        CERTIFICATE_TRANSFER_CALL.with(|call| *call.borrow_mut() = None);
        test::register_chain_extension(RecordingCertificateTransferExtension);

        let accounts = test::default_accounts::<CustomEnvironment>();
        let asset_id = [9u8; 32];
        let right_type = 1u8;
        let valid_until = Some(123_456u64);

        ink::EnvAccess::<CustomEnvironment>::default()
            .extension()
            .issue_certificate(asset_id, accounts.alice, accounts.bob, right_type, valid_until)
            .expect("certificate issue succeeds");

        let expected_payload =
            (asset_id, accounts.alice, accounts.bob, right_type, valid_until).encode();
        let expected_input = expected_payload.encode();
        CERTIFICATE_TRANSFER_CALL.with(|call| {
            assert_eq!(
                *call.borrow(),
                Some((ISSUE_CERT_FUNC_ID as u16, expected_input))
            );
        });
    }
}
