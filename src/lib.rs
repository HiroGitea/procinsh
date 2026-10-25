//! IPC values and pure analysis shared by the acquisition and HTTP executables.
//!
//! # Interface
//!
//! | Definition | Visibility | Kind / signature |
//! | --- | --- | --- |
//! | [`ipc`] | `pub` | `mod ipc` |
//! | [`model`] | `pub` | `mod model` |
//! | [`analysis`] | `pub` | `mod analysis` |

pub mod ipc;

pub mod model;

pub mod analysis;
