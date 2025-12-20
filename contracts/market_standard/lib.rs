#![cfg_attr(not(feature = "std"), no_std)]

use ink::primitives::AccountId;
use ink::env::Environment;
use codec::{Encode, Decode};
use scale_info::TypeInfo;

// 链扩展ID（u32类型）
pub const DATA_ASSETS_EXT_ID: u32 = 1;
pub const TRANSFER_ASSET_FUNC_ID: u32 = 1; // 方法ID
// 链扩展错误码
#[derive(Debug, Copy, Clone, PartialEq, Eq, Encode, Decode, TypeInfo)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub enum DataAssetsExtError {
    TransferFailed = 1,
    AssetNotFound = 2,
    PermissionDenied = 3,
}

// 为 DataAssetsExtError 实现 FromStatusCode trait
impl ink::env::chain_extension::FromStatusCode for DataAssetsExtError {
    fn from_status_code(status_code: u32) -> Result<(), Self> {
        match status_code {
            0 => Ok(()),
            1 => Err(Self::TransferFailed),
            2 => Err(Self::AssetNotFound),
            3 => Err(Self::PermissionDenied),
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