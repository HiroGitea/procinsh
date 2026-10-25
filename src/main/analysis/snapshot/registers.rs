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

pub use crate::ipc::registers::{Register, RegisterMapping, RegisterSet};
use crate::model::memory::{MemoryKind, MemoryMap};

#[derive(serde::Serialize)]
#[allow(dead_code)]
#[serde(remote = "crate::ipc::registers::RegisterMapping")]
struct RegisterMappingJson {
    pub pathname: Option<String>,
    pub readable: bool,
    pub writable: bool,
    pub executable: bool,
    pub private: bool,
}
impl serde::Serialize for crate::ipc::registers::RegisterMapping {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        RegisterMappingJson::serialize(self, s)
    }
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
#[derive(serde::Serialize)]
#[allow(dead_code)]
#[serde(remote = "crate::ipc::registers::Register")]
struct RegisterJson {
    pub name: String,
    #[serde(serialize_with = "crate::model::memory::hex")]
    pub value: u64,
    pub decimal: String,
    pub kind: MemoryKind,
    pub mapping: Option<RegisterMapping>,
    #[serde(serialize_with = "optional_hex")]
    pub offset: Option<u64>,
}
impl serde::Serialize for crate::ipc::registers::Register {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        RegisterJson::serialize(self, s)
    }
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
#[derive(serde::Serialize, serde::Deserialize)]
#[allow(dead_code)]
#[serde(remote = "crate::ipc::registers::RegisterSet")]
struct RegisterSetJson(pub [u64; 24]);
impl serde::Serialize for crate::ipc::registers::RegisterSet {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        RegisterSetJson::serialize(self, s)
    }
}

impl<'de> serde::Deserialize<'de> for crate::ipc::registers::RegisterSet {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        RegisterSetJson::deserialize(d)
    }
}
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
