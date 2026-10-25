//! Hardware values shared with main-side hardware observation.
//!
//! # Interface
//!
//! | Definition | Visibility | Kind / signature |
//! | --- | --- | --- |
//! | [`HardwareDevice`] | `pub(super)` | `struct HardwareDevice` |
//! | [`DeviceHandle`] | `pub(super)` | `struct DeviceHandle` |
//! | [`HardwareObservation`] | `pub(super)` | `struct HardwareObservation` |
//! | [`HardwareSources`] | `pub(super)` | `struct HardwareSources` |
//! | [`HardwarePath`] | `pub(super)` | `struct HardwarePath` |

pub(super) use procinsh_protocol::analysis::hardware::*;
