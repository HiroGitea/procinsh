//! Multiple hosts: a hub accepts agent links and relays each agent's snapshots and
//! activity to browsers; an agent links to a hub and pushes its own observations.
//!
//! Agent links use length-prefixed frames over TCP without TLS. A hub on a
//! non-loopback address requires a shared token. Agents collect only while the
//! hub has viewers for them.
//!
//! # Interface
//!
//! | Definition | Visibility | Kind / signature |
//! | --- | --- | --- |
//! | [`HostRegistry`] | `pub(super)` | `struct HostRegistry` |
//! | [`HostInfo`] | `pub(super)` | `struct HostInfo` |
//! | [`HostStatus`] | `pub(super)` | `enum HostStatus` |
//! | [`HostFeed`] | `pub(super)` | `struct HostFeed` |
//! | [`FeedSubscription`] | `pub(super)` | `struct FeedSubscription` |
//! | [`FeedEvent`] | `pub(super)` | `enum FeedEvent` |
//! | [`correlate`](fn@correlate) | `pub(super)` | `fn correlate(hosts: &[(`[`String`]`, `[`Map`](serde_json::Map)`<`[`String`]`, `[`Value`](serde_json::Value)`>)]) -> `[`correlate::CrossHostLinks`] |
//! | [`FeedSubscribeError`] | `pub(super)` | `enum FeedSubscribeError` |
//! | [`valid_host_name`] | `pub(crate)` | `fn valid_host_name(name: &`[`str`]`) -> `[`bool`] |
//! | [`run_agent_listener`] | `pub(super)` | `async fn run_agent_listener(listener: `[`TcpListener`](tokio::net::TcpListener)`, registry: `[`Arc`](std::sync::Arc)`<`[`HostRegistry`]`>, token: `[`Option`]`<`[`Arc`](std::sync::Arc)`<`[`str`]`>>, shutdown: `[`watch::Receiver`](tokio::sync::watch::Receiver)`<`[`bool`]`>)` |
//! | [`run_hub_connector`] | `pub(super)` | `async fn run_hub_connector(address: `[`String`]`, info: `[`HostInfo`]`, token: `[`Option`]`<`[`Arc`](std::sync::Arc)`<`[`str`]`>>, monitor: `[`Arc`](std::sync::Arc)`<`[`SystemMonitor`](super::system::SystemMonitor)`>, shutdown: `[`watch::Receiver`](tokio::sync::watch::Receiver)`<`[`bool`]`>)` |
//!
//! [`HostRegistry::summaries`] lists [`feed::HostSummary`].

mod connector;
mod correlate;
mod feed;
mod identity;
mod listener;
mod merge;
mod registry;
mod wire;
pub(super) use connector::run_hub_connector;
pub(super) use correlate::correlate;
pub(super) use feed::{FeedEvent, FeedSubscribeError, FeedSubscription, HostFeed, HostStatus};
pub(super) use identity::HostInfo;
pub(crate) use identity::valid_host_name;
pub(super) use listener::run_agent_listener;
pub(super) use registry::HostRegistry;
