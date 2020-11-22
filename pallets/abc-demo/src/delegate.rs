use crate::error::AbcError;
use crate::{APPLY_DELEGATE, SERVICE_BASE_URL};
use alt_serde::{Deserialize, Deserializer};
use codec::{Decode, Encode};
use frame_support::debug;
use sp_core::crypto::AccountId32;
use sp_runtime::offchain::storage::StorageValueRef;

const LOCAL_STORAGE_EMPLOYER_KEY_PREFIX: &'static str = "local-storage::employer-";
const LOCAL_STORAGE_EMPLOYER_LOCK_PREFIX: &'static str = "local-storage::employer-lock-";

#[serde(crate = "alt_serde")]
#[derive(Encode, Decode, Deserialize)]
pub struct DelegateInfo {
    #[serde(deserialize_with = "de_string_to_bytes")]
    pub delegator_tea_id: Vec<u8>,
    #[serde(deserialize_with = "de_string_to_bytes")]
    pub delegator_ephemeral_id: Vec<u8>,
    #[serde(deserialize_with = "de_string_to_bytes")]
    pub sig: Vec<u8>,
    #[serde(deserialize_with = "de_string_to_bytes")]
    pub key3_rsa_pub_key: Vec<u8>,
}

pub fn de_string_to_bytes<'de, D>(de: D) -> Result<Vec<u8>, D::Error>
where
    D: Deserializer<'de>,
{
    let s: &str = Deserialize::deserialize(de)?;
    Ok(s.as_bytes().to_vec())
}

pub fn request_single_delegate(account: AccountId32) {
    let employer = format!("{}", account);
    let request_url = format!(
        "{}{}?content={}",
        SERVICE_BASE_URL, APPLY_DELEGATE, &employer
    );

    match crate::http::http_post(&request_url) {
        Ok(resp) => {
            if let Err(e) = parse_delegate_response(resp, &employer) {
                debug::error!("parse delegate response failed: {}", e)
            }
        }
        Err(e) => debug::error!("delegate request failed: {}", e),
    }
}

fn parse_delegate_response(resp: Vec<u8>, employer: &str) -> anyhow::Result<()> {
    let resp_str = String::from_utf8(resp)?;
    let result_info: DelegateInfo = serde_json::from_str::<DelegateInfo>(&resp_str)
        .map_err(|e| AbcError::Common(format!("{}", e)))?;

    save_delegate_info(employer, &result_info)
}

fn save_delegate_info(employer: &str, delegate_info: &DelegateInfo) -> anyhow::Result<()> {
    operate_local_storage(employer, |value_ref| {
        value_ref.set(delegate_info);
        ()
    })
}

pub fn load_delegate_info(employer: &str) -> anyhow::Result<DelegateInfo> {
    match operate_local_storage(employer, |value_ref| value_ref.get::<DelegateInfo>()) {
        Ok(Some(Some(info))) => Ok(info),
        Err(e) => Err(anyhow::anyhow!(
            "get local storage about {} error, details: {}",
            employer,
            e
        )),
        _ => Err(anyhow::anyhow!("get local storage about {} error")),
    }
}

fn operate_local_storage<F, R>(employer: &str, mut callback: F) -> anyhow::Result<R>
where
    F: FnMut(StorageValueRef) -> R,
{
    let key = [LOCAL_STORAGE_EMPLOYER_KEY_PREFIX, employer]
        .concat()
        .as_bytes()
        .to_vec();
    let lock_key = [LOCAL_STORAGE_EMPLOYER_LOCK_PREFIX, employer]
        .concat()
        .as_bytes()
        .to_vec();
    let value_ref = StorageValueRef::persistent(&key);
    let lock = StorageValueRef::persistent(&lock_key);

    let res: Result<bool, bool> = lock.mutate(|s: Option<Option<bool>>| {
        match s {
            // `s` can be one of the following:
            //   `None`: the lock has never been set. Treated as the lock is free
            //   `Some(None)`: unexpected case, treated it as AlreadyFetch
            //   `Some(Some(false))`: the lock is free
            //   `Some(Some(true))`: the lock is held
            None | Some(Some(false)) => Ok(true),
            _ => Err(anyhow::anyhow!(
                "local storage key {} locked",
                String::from_utf8(key.clone())?
            )),
        }
    })?;

    match res {
        Ok(true) => {
            let rtn = callback(value_ref);
            lock.set(&false);
            Ok(rtn)
        }
        _ => Err(anyhow::anyhow!(
            "local storage key {} lock error",
            String::from_utf8(key)?
        )),
    }
}
