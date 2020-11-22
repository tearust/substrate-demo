use crate::delegate::{load_delegate_info, DelegateInfo};
use crate::http::http_post;
use crate::{Cid, ErrandId, SEND_ERRAND_TASK_ACTION, SERVICE_BASE_URL};
use frame_support::debug;

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
        // todo convert signature to hex string
        String::from_utf8(info.sig)?,
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
