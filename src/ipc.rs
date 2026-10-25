//! Shared IPC schema and bounded framing, without inspection implementations.
//!
//! # Interface
//! - [`schema`] (`pub`): `mod schema` defines process identities, closed requests, responses and events.
//! - [`transport`] (`pub`): `mod transport` provides bounded Unix stream read/write functions and deadlines.
//! - [`codec`] (`pub`): `mod codec` defines the native binary [`Wire`](codec::Wire) contract.
pub mod schema;
pub mod transport;

pub mod codec;
