use crate::delegate::{load_delegate_info, DelegateInfo};
use crate::error::AbcError;
use crate::http::http_post;
use crate::storage::operate_local_storage;
use crate::{
    Cid, ErrandId, ErrandResultInfo, LOCAL_STORAGE_TASKS_RESULTS_KEY,
    LOCAL_STORAGE_TASKS_RESULTS_LOCK, SERVICE_BASE_URL,
};
use frame_support::debug;
use sp_core::crypto::AccountId32;
use sp_core::Pair;

const QUERY_ERRAND_RESULT_ACTION: &'static str = "/api/query_errand_execution_result_by_uuid";
const SEND_ERRAND_TASK_ACTION: &'static str = "/api/service";

pub fn fetch_single_task_result(errand_id: &ErrandId, description_cid: &Cid) -> bool {
    match fetch_errand_result_info(errand_id, description_cid) {
        Ok(result) => result,
        Err(e) => {
            debug::error!("query_result_from_http error: {}", e);
            false
        }
    }
}

fn fetch_errand_result_info(errand_id: &ErrandId, description_cid: &Cid) -> anyhow::Result<bool> {
    let resp_bytes = http_query_task_result(errand_id)?;
    let resp_str = String::from_utf8(resp_bytes)?;
    let result_info: ErrandResultInfo = serde_json::from_str::<ErrandResultInfo>(&resp_str)
        .map_err(|e| AbcError::Common(format!("{}", e)))?;

    let key = LOCAL_STORAGE_TASKS_RESULTS_KEY.as_bytes().to_vec();
    let lock_key = LOCAL_STORAGE_TASKS_RESULTS_LOCK.as_bytes().to_vec();

    operate_local_storage(key, lock_key, |value_ref| {
        match value_ref.get::<Vec<(Cid, ErrandResultInfo)>>() {
            Some(Some(mut results)) => {
                results.push((description_cid.to_vec(), result_info.clone()));
                value_ref.set(&results);
            }
            _ => {
                let results = vec![(description_cid.to_vec(), result_info.clone())];
                value_ref.set(&results);
            }
        }
    })?;
    Ok(true)
}

fn http_query_task_result(errand_id: &ErrandId) -> anyhow::Result<Vec<u8>> {
    let request_url = format!(
        "{}{}/{}",
        SERVICE_BASE_URL,
        QUERY_ERRAND_RESULT_ACTION,
        String::from_utf8(errand_id.to_vec())?,
    );
    http_post(&request_url)
}

pub fn send_task_to_tea_network(
    employer: &str,
    description_cid: &Cid,
    errand_id: &ErrandId,
) -> bool {
    match send_task_internal(employer, description_cid, errand_id) {
        Ok(_) => true,
        Err(e) => {
            debug::error!("send_task_to_tea_network got error: {}", e);
            false
        }
    }
}

fn send_task_internal(
    employer: &str,
    description_cid: &Cid,
    errand_id: &ErrandId,
) -> anyhow::Result<()> {
    let info: DelegateInfo = load_delegate_info(employer)?;
    let cid = String::from_utf8(description_cid.to_vec())?;
    let request_url = format!(
        "{}{}/{}/{}/{}?content={}",
        SERVICE_BASE_URL,
        SEND_ERRAND_TASK_ACTION,
        employer,
        String::from_utf8(errand_id.to_vec())?,
        &hex::encode(info.sig),
        &cid,
    );
    let res = http_post(&request_url)?;

    debug::info!(
        "employer {} send task (cid {}) go response: {}",
        employer,
        cid,
        String::from_utf8(res)?
    );
    Ok(())
}

pub fn account_from_seed_in_accounts(seed: &str, accounts: Vec<AccountId32>) -> bool {
    let account = account_from_seed(seed);
    for ac in accounts {
        if ac.eq(&account) {
            return true;
        }
    }
    false
}

fn account_from_seed(seed: &str) -> AccountId32 {
    let public: [u8; 32] = sp_core::sr25519::Pair::from_string(&format!("//{}", seed), None)
        .expect("static values are valid; qed")
        .public()
        .into();
    AccountId32::from(public)
}
