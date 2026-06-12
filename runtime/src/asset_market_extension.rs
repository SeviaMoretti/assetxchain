// 数据资产扩展模块
// 与pallet-contracts交互，实现数据资产扩展，市场交易成功后，更新数据资产的状态

use log;
use pallet_contracts::chain_extension::{
    ChainExtension, Environment, Ext, InitState, RetVal, SysConfig,
};
use sp_core::crypto::UncheckedFrom;
use sp_runtime::DispatchError;

// 定义 Function IDs
const TRANSFER_ASSET_FUNC_ID: u16 = 1;
const TRANSFER_CERT_FUNC_ID: u16 = 2; // 新增：转移权证
const TRANSFER_FAILED_STATUS: u32 = 1;
const ASSET_NOT_FOUND_STATUS: u32 = 2;
const PERMISSION_DENIED_STATUS: u32 = 3;
const CERTIFICATE_NOT_FOUND_STATUS: u32 = 4;
const CERTIFICATE_NOT_ACTIVE_STATUS: u32 = 5;

fn dataassets_error_status<T: pallet_dataassets::Config>(error: DispatchError) -> u32 {
    if error == pallet_dataassets::Error::<T>::AssetNotFound.into() {
        ASSET_NOT_FOUND_STATUS
    } else if error == pallet_dataassets::Error::<T>::CertificateNotFound.into() {
        CERTIFICATE_NOT_FOUND_STATUS
    } else if error == pallet_dataassets::Error::<T>::CertificateNotActive.into() {
        CERTIFICATE_NOT_ACTIVE_STATUS
    } else if error == pallet_dataassets::Error::<T>::NotOwner.into()
        || error == pallet_dataassets::Error::<T>::NotAuthorized.into()
    {
        PERMISSION_DENIED_STATUS
    } else {
        TRANSFER_FAILED_STATUS
    }
}

#[derive(Default)]
pub struct DataAssetsExtension;

impl<T> ChainExtension<T> for DataAssetsExtension
where
    // T 必须配置了 pallet_contracts 和 pallet_dataassets
    T: pallet_contracts::Config + pallet_dataassets::Config,
    // 确保 AccountId 可以从 Hash 转换 (这是 pallet-contracts 要求的)
    <T as SysConfig>::AccountId: UncheckedFrom<<T as SysConfig>::Hash> + AsRef<[u8]>,
{
    fn call<E: Ext>(&mut self, env: Environment<E, InitState>) -> Result<RetVal, DispatchError>
    where
        E: Ext<T = T>,
    {
        let func_id = env.func_id();

        match func_id {
            // 交易元证
            TRANSFER_ASSET_FUNC_ID => {
                log::debug!(target: "runtime", "DataAssetsExtension: Calling TRANSFER_ASSET_FUNC_ID");
                let mut env = env.buf_in_buf_out();

                // 1. 读取输入 (AssetId: [u8; 32], To AccountId)，！！！！应该增加一个price
                // 之后调用 Incentive 模块：登记交易者（买家）月度交易额以及登记市场月度交易额
                let (asset_id_bytes, to_account): ([u8; 32], T::AccountId) = env.read_as()?;

                // 2. 获取调用合约的地址 (Contract Address)
                // 合约地址就是资产转移中的 Operator/Market
                let caller_account = env.ext().address().clone();

                // 3. 调用 pallet-dataassets 的内部函数
                // Runtime 会检查 caller_account (合约) 是否被授权
                pallet_dataassets::Pallet::<T>::transfer_by_market_internal(
                    &asset_id_bytes,
                    &caller_account,
                    &to_account,
                )?;

                // 4. 返回成功代码 0
                Ok(RetVal::Converging(0))
            }

            // 交易权证
            TRANSFER_CERT_FUNC_ID => {
                log::debug!(target: "runtime", "DataAssetsExtension: Calling TRANSFER_CERT_FUNC_ID");
                let mut env = env.buf_in_buf_out();
                let (asset_id, certificate_id, to_account): ([u8; 32], [u8; 32], T::AccountId) =
                    env.read_as()?;
                let caller_account = env.ext().address().clone();

                if pallet_dataassets::Pallet::<T>::get_certificate(&asset_id, &certificate_id)
                    .is_none()
                {
                    return Ok(RetVal::Converging(CERTIFICATE_NOT_FOUND_STATUS));
                }

                match pallet_dataassets::Pallet::<T>::transfer_certificate_internal(
                    &asset_id,
                    &certificate_id,
                    &caller_account,
                    &to_account,
                ) {
                    Ok(()) => Ok(RetVal::Converging(0)),
                    Err(error) => Ok(RetVal::Converging(dataassets_error_status::<T>(error))),
                }
            }
            _ => Err(DispatchError::Other("Unregistered function")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn certificate_transfer_status_codes_are_explicit_errors() {
        assert_ne!(CERTIFICATE_NOT_FOUND_STATUS, 0);
        assert_ne!(CERTIFICATE_NOT_ACTIVE_STATUS, 0);
        assert_ne!(TRANSFER_FAILED_STATUS, CERTIFICATE_NOT_FOUND_STATUS);
        assert_ne!(CERTIFICATE_NOT_FOUND_STATUS, CERTIFICATE_NOT_ACTIVE_STATUS);
    }
}
