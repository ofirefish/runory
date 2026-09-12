//! HashiCorp Boundary BastionProvider — uses official `boundary` CLI local proxy.

mod auth;
mod cli;
mod provider;

pub use provider::BoundaryProvider;
