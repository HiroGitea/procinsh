//! Shared native IPC data, codec and bounded Unix-stream framing. JSON conversion belongs to main.
//!
//! # Interface
//! - [`schema`] (`pub`): `mod schema`.
//! - [`transport`] (`pub`): `mod transport`.
//! - [`codec`] (`pub`): `mod codec`.
//! - [`model`] (`pub`): `mod model`.
//! - [`activity`] (`pub`): `mod activity`.
//! - [`descriptors`] (`pub`): `mod descriptors`.
//! - [`hardware`] (`pub`): `mod hardware`.
//! - [`history`] (`pub`): `mod history`.
//! - [`registers`] (`pub`): `mod registers`.
//! - [`stack`] (`pub`): `mod stack`.
//! - [`acquisition`] (`pub`): `mod acquisition`.
//! - [`elf`] (`pub`): `mod elf`.
pub mod schema;
pub mod transport;

pub mod codec;

#[path = "model/root.rs"]
pub mod model;

pub mod activity;
pub mod descriptors;
pub mod hardware;
pub mod history;
pub mod registers;
pub mod stack;

pub mod acquisition;

pub mod elf;
