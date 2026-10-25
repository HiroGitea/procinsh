//! Typed identities shared by process discovery and activity collection.
//!
//! File and IPC inodes serialize as decimal strings to preserve integer precision.
//!
//! # Interface
//!
//! | Definition | Visibility | Kind / signature |
//! | --- | --- | --- |
//! | [`deserialize_decimal`] | `pub` | `fn deserialize_decimal<'de, D: serde::Deserializer<'de>>( deserializer: D, ) -> Result<u64, D::Error>` |
//! | [`DeviceId`] | `pub` | `struct DeviceId` |
//! | [`decimal`] | `pub` | `fn decimal<S: serde::Serializer>(value: &u64, serializer: S) -> Result<S::Ok, S::Error>` |
//! | [`IpcKind`] | `pub` | `enum IpcKind` |
//! | [`IpcIdentity`] | `pub` | `struct IpcIdentity` |
//! | [`FileIdentity`] | `pub` | `struct FileIdentity` |

use serde::{Deserialize, Serialize};
use std::fmt;

pub fn deserialize_decimal<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<u64, D::Error> {
    let value = String::deserialize(deserializer)?;
    value.parse().map_err(serde::de::Error::custom)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DeviceId {
    pub major: u32,
    pub minor: u32,
}
impl DeviceId {
    pub fn from_stat(device: u64) -> Self {
        Self {
            major: libc::major(device),
            minor: libc::minor(device),
        }
    }

    pub fn from_kernel(device: u64) -> Self {
        Self {
            major: (device >> 20) as u32,
            minor: (device & ((1 << 20) - 1)) as u32,
        }
    }
}

pub fn decimal<S: serde::Serializer>(value: &u64, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.collect_str(value)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IpcKind {
    Pipe,
    Socket,
}
impl fmt::Display for IpcKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Pipe => "pipe",
            Self::Socket => "socket",
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct IpcIdentity {
    pub kind: IpcKind,
    pub device: DeviceId,
    #[serde(serialize_with = "decimal", deserialize_with = "deserialize_decimal")]
    pub inode: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FileIdentity {
    pub device: DeviceId,
    #[serde(serialize_with = "decimal", deserialize_with = "deserialize_decimal")]
    pub inode: u64,
    pub generation: u32,
}

crate::ipc::codec::wire_struct!(DeviceId { major, minor });
crate::ipc::codec::wire_struct!(IpcIdentity {
    kind,
    device,
    inode
});
crate::ipc::codec::wire_struct!(FileIdentity {
    device,
    inode,
    generation
});
crate::ipc::codec::wire_enum!(IpcKind { 0 => Pipe, 1 => Socket });

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn devices_and_large_inodes_keep_identity() {
        let device = DeviceId {
            major: 259,
            minor: 1048575,
        };
        assert_eq!(
            DeviceId::from_stat(libc::makedev(device.major, device.minor)),
            device
        );
        assert_eq!(
            DeviceId::from_kernel(((device.major as u64) << 20) | device.minor as u64),
            device
        );
        let file = FileIdentity {
            device,
            inode: u64::MAX,
            generation: 42,
        };
        assert_eq!(
            serde_json::to_value(file).unwrap()["inode"],
            u64::MAX.to_string()
        );
        let mut identities = std::collections::HashSet::from([file]);
        identities.insert(FileIdentity {
            inode: file.inode - 1,
            ..file
        });
        identities.insert(FileIdentity {
            generation: file.generation + 1,
            ..file
        });
        identities.insert(FileIdentity {
            device: DeviceId {
                major: device.major + 1,
                ..device
            },
            ..file
        });
        identities.insert(FileIdentity {
            device: DeviceId {
                minor: device.minor - 1,
                ..device
            },
            ..file
        });
        assert_eq!(identities.len(), 5);
        let ipc = IpcIdentity {
            kind: IpcKind::Socket,
            device,
            inode: u64::MAX,
        };
        assert_eq!(
            serde_json::to_value(ipc).unwrap()["inode"],
            u64::MAX.to_string()
        );
        let mut ipc_keys = std::collections::HashSet::from([ipc]);
        ipc_keys.insert(IpcIdentity {
            kind: IpcKind::Pipe,
            ..ipc
        });
        ipc_keys.insert(IpcIdentity {
            inode: ipc.inode - 1,
            ..ipc
        });
        ipc_keys.insert(IpcIdentity {
            device: DeviceId {
                major: device.major + 1,
                ..device
            },
            ..ipc
        });
        ipc_keys.insert(IpcIdentity {
            device: DeviceId {
                minor: device.minor - 1,
                ..device
            },
            ..ipc
        });
        assert_eq!(ipc_keys.len(), 5);
    }
}
