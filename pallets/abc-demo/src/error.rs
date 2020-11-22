use thiserror::Error;

#[derive(Debug, Error)]
pub enum AbcError {
    #[error("http request failed, details: `{0}`")]
    HttpRequestError(String),

    #[error("http response failed, details: `{0}`")]
    HttpResponseError(String),

    #[error("http response got error code: `{0}`")]
    HttpResponseErrorCode(u16),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
