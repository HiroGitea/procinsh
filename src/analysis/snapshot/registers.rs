//! Registers.
//!
//! # Interface
//!
//! | Definition | Visibility | Kind / signature |
//! | --- | --- | --- |
//! | [`RegisterMapping`] | `pub` | `struct RegisterMapping` |
//! | [`Register`] | `pub` | `struct Register` |
//! | [`classify`] | `pub` | `fn classify(name: &str, value: u64, maps: &[MemoryMap]) -> Register` |
//! | [`RegisterSet`] | `pub` | `struct RegisterSet` |
//! | [`from_sample`] | `pub` | `fn from_sample(r: &RegisterSet, maps: &[MemoryMap]) -> Vec<Register>` |
//!
//! Mapping arguments use [`MemoryMap`]; classification values use [`MemoryKind`].

use crate::model::memory::{MemoryKind, MemoryMap};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct RegisterMapping {
    pub pathname: Option<String>,
    pub readable: bool,
    pub writable: bool,
    pub executable: bool,
    pub private: bool,
}

fn optional_hex<S: serde::Serializer>(
    value: &Option<u64>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match value {
        Some(value) => serializer.serialize_some(&format!("0x{value:x}")),
        None => serializer.serialize_none(),
    }
}
/// Classified register value; its mapping-relative offset serializes as hex or null.
#[derive(Clone, Debug, Serialize)]
pub struct Register {
    pub name: String,
    #[serde(serialize_with = "crate::model::memory::hex")]
    pub value: u64,
    pub decimal: String,
    pub kind: MemoryKind,
    pub mapping: Option<RegisterMapping>,
    #[serde(serialize_with = "optional_hex")]
    pub offset: Option<u64>,
}

pub fn classify(name: &str, value: u64, maps: &[MemoryMap]) -> Register {
    let map = maps.iter().find(|m| m.contains(value));
    Register {
        name: name.into(),
        value,
        decimal: value.to_string(),
        kind: map.map_or(MemoryKind::Integer, MemoryMap::kind),
        mapping: map.map(|m| RegisterMapping {
            pathname: m.pathname.clone(),
            readable: m.readable,
            writable: m.writable,
            executable: m.executable,
            private: m.private,
        }),
        offset: map.map(|m| value - m.start),
    }
}

/// Values indexed by Linux PERF_REG_X86_* (unused segment registers are zero).
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct RegisterSet(pub [u64; 24]);
pub fn from_sample(r: &RegisterSet, maps: &[MemoryMap]) -> Vec<Register> {
    [
        ("RIP", 8),
        ("RSP", 7),
        ("RBP", 6),
        ("RAX", 0),
        ("RBX", 1),
        ("RCX", 2),
        ("RDX", 3),
        ("RSI", 4),
        ("RDI", 5),
        ("R8", 16),
        ("R9", 17),
        ("R10", 18),
        ("R11", 19),
        ("R12", 20),
        ("R13", 21),
        ("R14", 22),
        ("R15", 23),
        ("RFLAGS", 9),
    ]
    .into_iter()
    .map(|(name, index)| classify(name, r.0[index], maps))
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_mapping_keeps_fields_and_offset_or_integer() {
        let map =
            crate::model::memory::parse_map("1000-2000 rw-p 00000000 08:01 42 /tmp/a [b]").unwrap();
        let register = classify("RAX", 0x1008, &[map]);
        assert_eq!(register.kind, MemoryKind::File);
        assert_eq!(register.offset, Some(8));
        let json = serde_json::to_value(register).unwrap();
        assert_eq!(
            json["mapping"],
            serde_json::json!({"pathname":"/tmp/a [b]","readable":true,"writable":true,"executable":false,"private":true})
        );
        assert_eq!(json["offset"], "0x8");
        let integer = classify("RAX", u64::MAX, &[]);
        assert_eq!(integer.kind, MemoryKind::Integer);
        assert!(integer.mapping.is_none() && integer.offset.is_none());
        assert_eq!(
            serde_json::to_value(integer).unwrap()["offset"],
            serde_json::Value::Null
        );
    }
}
