use crate::error::AbcError;
use alt_serde::Deserialize;
use codec::{Decode, Encode};
use frame_support::debug;
use sp_core::offchain::HttpError;
use sp_runtime::offchain::{self as rt_offchain};

const USER_AGENT: &'static str = "tearust";
const HTTP_POST_TIMEOUT: u64 = 180000; // post timeout set to 3 minutes.

#[serde(crate = "alt_serde")]
#[derive(Encode, Decode, Deserialize, Clone)]
struct ResponseResult {
    data: String,
}

pub fn http_post(url: &str) -> anyhow::Result<Vec<u8>> {
    let post_body = vec![b"post body"];

    debug::info!("begin to send http post request, url is {}", url);
    let request = rt_offchain::http::Request::post(url, post_body);
    let timeout =
        sp_io::offchain::timestamp().add(rt_offchain::Duration::from_millis(HTTP_POST_TIMEOUT));
    let pending = request
        .add_header("User-Agent", USER_AGENT)
        .deadline(timeout)
        .send()
        .map_err(|e| match e {
            HttpError::DeadlineReached => AbcError::HttpRequestError(
                "The requested action couldn't been completed within a deadline".into(),
            ),
            HttpError::IoError => AbcError::HttpRequestError(
                "There was an IO Error while processing the request".into(),
            ),
            HttpError::Invalid => {
                AbcError::HttpRequestError("ID of the request is invalid in this context".into())
            }
        })?;

    let response = pending
        .try_wait(timeout)
        .map_err(|e| AbcError::HttpResponseError(format!("{}", e.id.0)))?
        .map_err(|_| AbcError::HttpResponseError("unknown request error".into()))?;

    if response.code != 200 {
        return Err(anyhow::anyhow!(
            "Unexpected http request status code: {}",
            response.code
        ));
    }

    let res_body = String::from_utf8(response.body().collect::<Vec<u8>>())?;
    let response_result: ResponseResult = serde_json::from_str::<ResponseResult>(&res_body)
        .map_err(|e| AbcError::Common(format!("{}", e)))?;
    debug::info!(
        "end of http request ({}), response is {}",
        url,
        &response_result.data,
    );
    Ok(response_result.data.as_bytes().to_vec())
}
