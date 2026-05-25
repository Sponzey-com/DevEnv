use std::fmt;

use crate::{CatalogTrustFailure, DomainError};

pub type CoreResult<T> = Result<T, CoreError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreError {
    Domain(DomainError),
    CatalogTrust(CatalogTrustFailure),
    CatalogNetwork(String),
    Message(String),
}

impl CoreError {
    pub fn message(value: impl Into<String>) -> Self {
        Self::Message(value.into())
    }

    pub fn catalog_trust(value: CatalogTrustFailure) -> Self {
        Self::CatalogTrust(value)
    }

    pub fn catalog_network(value: impl Into<String>) -> Self {
        Self::CatalogNetwork(value.into())
    }
}

impl fmt::Display for CoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Domain(error) => write!(formatter, "{error}"),
            Self::CatalogTrust(error) => write!(formatter, "catalog trust failure: {error}"),
            Self::CatalogNetwork(message) => {
                write!(formatter, "catalog network failure: {message}")
            }
            Self::Message(message) => formatter.write_str(message),
        }
    }
}

impl From<CatalogTrustFailure> for CoreError {
    fn from(value: CatalogTrustFailure) -> Self {
        Self::CatalogTrust(value)
    }
}

impl std::error::Error for CoreError {}

impl From<DomainError> for CoreError {
    fn from(value: DomainError) -> Self {
        Self::Domain(value)
    }
}
