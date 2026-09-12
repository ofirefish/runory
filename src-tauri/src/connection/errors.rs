use thiserror::Error;

use super::bastion::BastionError;

/// Errors produced while resolving a host connection route.
#[derive(Debug, Error)]
pub enum ConnectionError {
    #[error("profile was not found")]
    ProfileNotFound,
    #[error("jump host configuration is invalid")]
    InvalidJumpHost,
    #[error("bastion provider was not found")]
    BastionProviderNotFound,
    #[error("bastion route is not executable yet")]
    BastionUnavailable,
    #[error(transparent)]
    Bastion(#[from] BastionError),
}

impl ConnectionError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::ProfileNotFound => "PROFILE_NOT_FOUND",
            Self::InvalidJumpHost => "INVALID_JUMP_HOST",
            Self::BastionProviderNotFound => "BASTION_PROVIDER_NOT_FOUND",
            Self::BastionUnavailable => "BASTION_UNAVAILABLE",
            Self::Bastion(error) => error.code(),
        }
    }
}
