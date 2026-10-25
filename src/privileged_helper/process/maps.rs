//! Read bounded process mapping observations.
//!
//! # Interface
//!
//! | Definition | Visibility | Kind / signature |
//! | --- | --- | --- |
//! | [`read_maps`] | `pub(super)` | `fn read_maps(pid: i32) -> Result<Vec<MemoryMap>>` |
//! | [`read_smaps`] | `pub(super)` | `fn read_smaps(pid: i32) -> Result<Vec<SmapsEntry>>` |
//! | [`MemoryRollup`] | `pub(super)` | `struct MemoryRollup` |
//! | [`rollup`] | `pub(super)` | `fn rollup(pid: i32) -> Option<MemoryRollup>` |
//! | [`deserialize_hex`] | `pub(in crate::privileged_helper)` | `fn deserialize_hex<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error>` |
//! | [`MemoryKind`] | `pub(in crate::privileged_helper)` | `enum MemoryKind` |
//! | [`MappingSnapshot`] | `pub(in crate::privileged_helper)` | `struct MappingSnapshot` |
//! | [`MemoryMap`] | `pub(in crate::privileged_helper)` | `struct MemoryMap` |
//! | [`SmapsEntry`] | `pub(in crate::privileged_helper)` | `struct SmapsEntry` |
//! | [`hex`] | `pub(in crate::privileged_helper)` | `fn hex<S: serde::Serializer>(value: &u64, serializer: S) -> Result<S::Ok, S::Error>` |
//! | [`parse_map`] | `pub(in crate::privileged_helper)` | `fn parse_map(line: &str) -> Result<MemoryMap>` |
//! | [`parse_smaps`] | `pub(in crate::privileged_helper)` | `fn parse_smaps(text: &str) -> Result<Vec<SmapsEntry>>` |

use super::procfs::read_text;
use anyhow::Result;
pub(in crate::privileged_helper) use procinsh_protocol::model::memory::*;
use serde::Serialize;
pub(super) fn read_maps(pid: i32) -> Result<Vec<MemoryMap>> {
    read_text(&format!("/proc/{pid}/maps"))?
        .lines()
        .map(parse_map)
        .collect()
}

/// Reads smaps, falling back to maps without usage measurements if smaps cannot be read.
pub(super) fn read_smaps(pid: i32) -> Result<Vec<SmapsEntry>> {
    if let Ok(text) = read_text(&format!("/proc/{pid}/smaps")) {
        return parse_smaps(&text);
    }
    Ok(read_maps(pid)?.into_iter().map(SmapsEntry::from).collect())
}

#[derive(Clone, Debug, Default, Serialize)]
pub(super) struct MemoryRollup {
    pub(super) rss_bytes: Option<u64>,
    pub(super) pss_bytes: Option<u64>,
    pub(super) private_bytes: Option<u64>,
}

pub(super) fn rollup(pid: i32) -> Option<MemoryRollup> {
    let f = super::procfs::fields(&format!("/proc/{pid}/smaps_rollup")).ok()?;
    let bytes = |k| super::procfs::field_u64(&f, k).and_then(|v| v.checked_mul(1024));
    Some(MemoryRollup {
        rss_bytes: bytes("Rss"),
        pss_bytes: bytes("Pss"),
        private_bytes: bytes("Private_Clean")
            .zip(bytes("Private_Dirty"))
            .map(|(a, b)| a + b),
    })
}
