//! Shared resource values used during acquisition.
//!
//! # Interface
//!
//! | Definition | Visibility | Kind / signature |
//! | --- | --- | --- |
//! | [`deserialize_decimal`] | `pub(super)` | `fn deserialize_decimal<'de, D: serde::Deserializer<'de>>( deserializer: D, ) -> Result<u64, D::Error>` |
//! | [`DeviceId`] | `pub(super)` | `struct DeviceId` |
//! | [`decimal`] | `pub(super)` | `fn decimal<S: serde::Serializer>(value: &u64, serializer: S) -> Result<S::Ok, S::Error>` |
//! | [`IpcKind`] | `pub(super)` | `enum IpcKind` |
//! | [`IpcIdentity`] | `pub(super)` | `struct IpcIdentity` |
//! | [`FileIdentity`] | `pub(super)` | `struct FileIdentity` |

pub(super) use procinsh_protocol::model::resource::*;
