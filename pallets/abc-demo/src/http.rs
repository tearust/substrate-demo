use crate::error::AbcError;
use frame_support::debug;
use sp_core::offchain::HttpError;
use sp_runtime::offchain::{self as rt_offchain};

const USER_AGENT: &'static str = "tearust";

pub fn http_post(url: &str) -> anyhow::Result<Vec<u8>> {
    let post_body = vec![b"post body"];

    debug::info!("begin to send http post request, url is {}", url);
    let request = rt_offchain::http::Request::post(url, post_body);
    let timeout = sp_io::offchain::timestamp().add(rt_offchain::Duration::from_millis(3000));
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

    let res_body = response.body().collect::<Vec<u8>>();
    debug::info!(
        "end of http request ({}), response is {}",
        url,
        String::from_utf8(res_body.clone())?
    );
    Ok(res_body)
}
