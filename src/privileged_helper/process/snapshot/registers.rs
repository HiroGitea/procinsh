//! Shared captured registers values.
//!
//! # Interface
//!
//! | Definition | Visibility | Kind / signature |
//! | --- | --- | --- |
//! | [`RegisterMapping`] | `pub(super)` | `struct RegisterMapping` |
//! | [`Register`] | `pub(super)` | `struct Register` |
//! | [`classify`] | `pub(super)` | `fn classify(name: &str, value: u64, maps: &[MemoryMap]) -> Register` |
//! | [`RegisterSet`] | `pub(super)` | `struct RegisterSet` |
//! | [`from_sample`] | `pub(super)` | `fn from_sample(r: &RegisterSet, maps: &[MemoryMap]) -> Vec<Register>` |

pub(super) use procinsh_protocol::analysis::snapshot::registers::*;
