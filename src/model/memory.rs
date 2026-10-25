//! Mapping values, classification and parsing of captured maps and smaps.
//!
//! # Interface
//!
//! | Definition | Visibility | Kind / signature |
//! | --- | --- | --- |
//! | [`deserialize_hex`] | `pub` | `fn deserialize_hex<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error>` |
//! | [`MemoryKind`] | `pub` | `enum MemoryKind` |
//! | [`MemoryMap`] | `pub` | `struct MemoryMap` |
//! | [`MappingSnapshot`] | `pub` | `struct MappingSnapshot` |
//! | [`SmapsEntry`] | `pub` | `struct SmapsEntry` |
//! | [`hex`] | `pub` | `fn hex<S: serde::Serializer>(value: &u64, serializer: S) -> Result<S::Ok, S::Error>` |
//! | [`parse_map`] | `pub` | `fn parse_map(line: &str) -> Result<MemoryMap>` |
//! | [`parse_smaps`] | `pub` | `fn parse_smaps(text: &str) -> Result<Vec<SmapsEntry>>` |

use crate::model::resource::{DeviceId, decimal};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
pub fn deserialize_hex<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
    let value = String::deserialize(deserializer)?;
    u64::from_str_radix(value.trim_start_matches("0x"), 16).map_err(serde::de::Error::custom)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    Integer,
    Stack,
    Heap,
    SharedLibrary,
    Executable,
    File,
    Anonymous,
}
/// Structural mapping without RSS/PSS measurements.
/// Addresses and file offsets serialize as hexadecimal strings; inodes as decimal strings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryMap {
    #[serde(serialize_with = "hex", deserialize_with = "deserialize_hex")]
    pub start: u64,
    #[serde(serialize_with = "hex", deserialize_with = "deserialize_hex")]
    pub end: u64,
    pub readable: bool,
    pub writable: bool,
    pub executable: bool,
    pub private: bool,
    #[serde(serialize_with = "hex", deserialize_with = "deserialize_hex")]
    pub file_offset: u64,
    pub device: DeviceId,
    #[serde(
        serialize_with = "decimal",
        deserialize_with = "crate::model::resource::deserialize_decimal"
    )]
    pub inode: u64,
    pub pathname: Option<String>,
}

/// A mapping with optional resident/proportional usage measured from smaps.
/// Serialization flattens the structural mapping alongside the usage measurements.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SmapsEntry {
    #[serde(flatten)]
    pub mapping: MemoryMap,
    pub rss_bytes: Option<u64>,
    pub pss_bytes: Option<u64>,
}
impl From<MemoryMap> for SmapsEntry {
    fn from(mapping: MemoryMap) -> Self {
        Self {
            mapping,
            rss_bytes: None,
            pss_bytes: None,
        }
    }
}

pub fn hex<S: serde::Serializer>(value: &u64, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&format!("0x{value:016x}"))
}
impl MemoryMap {
    pub fn contains(&self, address: u64) -> bool {
        self.start <= address && address < self.end
    }

    pub fn kind(&self) -> MemoryKind {
        match self.pathname.as_deref() {
            Some(p) if p.starts_with("[stack") => MemoryKind::Stack,
            Some("[heap]") => MemoryKind::Heap,
            Some(p) if p.contains(".so") => MemoryKind::SharedLibrary,
            _ if self.executable => MemoryKind::Executable,
            Some(p) if p.starts_with('/') => MemoryKind::File,
            _ => MemoryKind::Anonymous,
        }
    }
}

pub fn parse_map(line: &str) -> Result<MemoryMap> {
    // Split only the first five columns: file names may contain spaces.
    let mut rest = line.trim();
    let mut columns = Vec::new();
    for _ in 0..5 {
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        ensure!(end > 0, "maps: missing field");
        columns.push(&rest[..end]);
        rest = rest[end..].trim_start();
    }
    let (start, end) = columns[0].split_once('-').context("maps: invalid range")?;
    let permissions = columns[1];
    ensure!(permissions.len() == 4, "maps: invalid permissions");
    let start = u64::from_str_radix(start, 16)?;
    let end = u64::from_str_radix(end, 16)?;
    ensure!(start < end, "maps: empty range");
    Ok(MemoryMap {
        start,
        end,
        readable: permissions.as_bytes()[0] == b'r',
        writable: permissions.as_bytes()[1] == b'w',
        executable: permissions.as_bytes()[2] == b'x',
        private: permissions.as_bytes()[3] == b'p',
        file_offset: u64::from_str_radix(columns[2], 16)?,
        device: {
            let (major, minor) = columns[3].split_once(':').context("maps: invalid device")?;
            DeviceId {
                major: u32::from_str_radix(major, 16)?,
                minor: u32::from_str_radix(minor, 16)?,
            }
        },
        inode: columns[4].parse()?,
        pathname: (!rest.is_empty()).then(|| rest.to_owned()),
    })
}

pub fn parse_smaps(text: &str) -> Result<Vec<SmapsEntry>> {
    let mut maps: Vec<SmapsEntry> = Vec::new();
    for line in text.lines() {
        if line
            .split_whitespace()
            .next()
            .is_some_and(|word| word.contains('-'))
        {
            maps.push(parse_map(line)?.into());
        } else if let Some(map) = maps.last_mut()
            && let Some((key, value)) = line.split_once(':')
        {
            let bytes = value
                .split_whitespace()
                .next()
                .and_then(|v| v.parse::<u64>().ok())
                .and_then(|v| v.checked_mul(1024));
            match key {
                "Rss" => map.rss_bytes = bytes,
                "Pss" => map.pss_bytes = bytes,
                _ => {}
            }
        }
    }
    Ok(maps)
}

/// One captured mapping context shared by all thread samples from that epoch.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MappingSnapshot {
    pub epoch: u64,
    pub maps: Vec<MemoryMap>,
}

crate::ipc::codec::wire_struct!(MemoryMap {
    start,
    end,
    readable,
    writable,
    executable,
    private,
    file_offset,
    device,
    inode,
    pathname
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_and_smaps_keep_paths_and_boundaries() {
        let maps = parse_smaps("1000-2000 r-xp 00001000 08:01 10 /tmp/a b (deleted)\nRss: 4 kB\nPss: 2 kB\n2000-3000 rw-p 0 00:00 0\n").unwrap();
        assert_eq!(
            maps[0].mapping.pathname.as_deref(),
            Some("/tmp/a b (deleted)")
        );
        assert_eq!(maps[0].rss_bytes, Some(4096));
        let detailed = serde_json::to_value(&maps[0]).unwrap();
        assert_eq!(detailed["rss_bytes"], 4096);
        assert_eq!(detailed["pss_bytes"], 2048);
        assert_eq!(detailed["start"], "0x0000000000001000");
        assert!(detailed.get("mapping").is_none());
        assert!(maps[0].mapping.contains(0x1000));
        assert!(!maps[0].mapping.contains(0x2000));
        assert!(maps[1].mapping.pathname.is_none());
        assert!(parse_map("2000-1000 rw-p 0 00:00 0").is_err());
    }

    #[test]
    fn structural_maps_exclude_usage_and_fallback_observations_keep_shape() {
        let mapping = parse_map("1000-2000 r-xp 0 00:00 0 /tmp/test").unwrap();
        let structural = serde_json::to_value(&mapping).unwrap();
        assert!(structural.get("rss_bytes").is_none());
        assert!(structural.get("pss_bytes").is_none());
        let fallback = SmapsEntry::from(mapping.clone());
        let json = serde_json::to_value(&fallback).unwrap();
        assert!(json["rss_bytes"].is_null());
        assert!(json["pss_bytes"].is_null());
        assert_eq!(json["pathname"], "/tmp/test");
        let measured =
            parse_smaps("1000-2000 r-xp 0 00:00 0 /tmp/test\nRss: 8 kB\nPss: 4 kB\n").unwrap();
        assert_eq!(measured[0].mapping, mapping);
        assert_eq!(measured[0].rss_bytes, Some(8192));
    }
}
