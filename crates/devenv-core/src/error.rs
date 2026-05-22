use std::fmt;

use crate::DomainError;

pub type CoreResult<T> = Result<T, CoreError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreError {
    Domain(DomainError),
    Message(String),
}

impl CoreError {
    pub fn message(value: impl Into<String>) -> Self {
        Self::Message(value.into())
    }
}

impl fmt::Display for CoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Domain(error) => write!(formatter, "{error}"),
            Self::Message(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for CoreError {}

impl From<DomainError> for CoreError {
    fn from(value: DomainError) -> Self {
        Self::Domain(value)
    }
}
