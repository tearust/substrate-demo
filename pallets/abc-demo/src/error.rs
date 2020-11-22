use thiserror::Error;

#[derive(Debug, Error)]
pub enum AbcError {
    #[error("abc common error, details: `{0}`")]
    Common(String),

    #[error("http request failed, details: `{0}`")]
    HttpRequestError(String),

    #[error("http response failed, details: `{0}`")]
    HttpResponseError(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
