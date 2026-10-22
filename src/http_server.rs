//! HTTP server wiring and shared API.
//!
//! # Interface
//!
//! | Definition | Visibility | Signature |
//! | --- | --- | --- |
//! | [`run`] | `pub(super)` | `async fn run(config: `[`ServerConfig`]`) -> `[`anyhow::Result`]`<()>` |
//! | [`ServerConfig`] | `pub(super)` | `struct ServerConfig` |
//! | [`valid_host_name`] | `pub(super)` | `fn valid_host_name(name: &`[`str`]`) -> `[`bool`] |
//!
//! Shared resource identities live in [`resource`]; socket metadata and descriptor
//! capabilities live in [`socket_types`]; bounded netlink dumps live in [`netlink`].
//!
mod api;
mod hosts;
mod middleware;
mod netlink;
mod process;
mod resource;
mod router;
mod server;
mod socket_types;
mod state;
mod system;
mod web;
pub(super) use hosts::valid_host_name;
pub(super) use server::{ServerConfig, run};
use state::AppState;
#[cfg(test)]
mod hosts_link_tests;
#[cfg(test)]
mod process_api_tests;
#[cfg(test)]
mod system_api_tests;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;
