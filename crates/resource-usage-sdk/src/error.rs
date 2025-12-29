// src/error.rs
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("sdk error:{0:?}")]
    SDKClientError(#[from] soroban_client::error::Error),

    #[error("sdk xdr error:{0:?}")]
    SDKXdrError(#[from] soroban_client::xdr::Error),

    #[error("missing transaction meta")]
    MissingMeta,

    #[error("unsupported transaction meta version")]
    UnsupportedMeta,

    #[error("simulate no transaction data")]
    NoTransactionData,
}
