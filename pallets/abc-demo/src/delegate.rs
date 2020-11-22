use crate::error::AbcError;
use crate::{DelegateInfo, APPLY_DELEGATE, SERVICE_BASE_URL};
use frame_support::debug;
use sp_core::crypto::AccountId32;

pub fn request_single_delegate(account: AccountId32) {
    let request_url = format!("{}{}?content={}", SERVICE_BASE_URL, APPLY_DELEGATE, account);

    match crate::http::http_post(&request_url) {
        Ok(resp) => {
            if let Err(e) = parse_delegate_response(resp) {
                debug::error!("parse delegate response failed: {}", e)
            }
        }
        Err(e) => debug::error!("delegate request failed: {}", e),
    }
}

fn parse_delegate_response(resp: Vec<u8>) -> anyhow::Result<()> {
    let resp_str = String::from_utf8(resp)?;
    let result_info: DelegateInfo = serde_json::from_str::<DelegateInfo>(&resp_str)
        .map_err(|e| AbcError::Common(format!("{}", e)))?;
    //todo call save_delegate_info
    // Self::save_delegate_info(
    //     str::from_utf8(&employer.encode()).map_err(|_| Error::<T>::ApplyDelegateError)?,
    //     &result_info,
    // )?;
    Ok(())
}
