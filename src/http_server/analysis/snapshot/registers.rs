//! Registers.
//!
//! # Interface
//!
//! | Definition | Visibility | Kind / signature |
//! | --- | --- | --- |
//! | [`RegisterMapping`] | `pub` | `struct RegisterMapping` |
//! | [`Register`] | `pub` | `struct Register` |
//! | [`classify`] | `pub` | `fn classify(name: &str, role: RegisterRole, value: u64, maps: &[MemoryMap]) -> Register` |
//! | [`RegisterRole`] | `pub` | `enum RegisterRole` |
//! | [`RegisterSet`] | `pub` | `struct RegisterSet` |
//! | [`from_sample`] | `pub` | `fn from_sample(r: &RegisterSet, maps: &[MemoryMap]) -> Vec<Register>` |
//!
//! Mapping arguments use [`MemoryMap`]; classification values use [`MemoryKind`].

use crate::http_server::model::memory::{MemoryKind, MemoryMap};
pub use crate::ipc::registers::{Register, RegisterMapping, RegisterRole, RegisterSet};

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
    pub role: RegisterRole,
    #[serde(serialize_with = "crate::http_server::model::memory::hex")]
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

pub fn classify(name: &str, role: RegisterRole, value: u64, maps: &[MemoryMap]) -> Register {
    let map = maps.iter().find(|m| m.contains(value));
    Register {
        name: name.into(),
        role,
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
    REGISTERS
        .iter()
        .map(|&(name, role, index)| classify(name, role, r.0[index], maps))
        .collect()
}

#[cfg(target_arch = "x86_64")]
const REGISTERS: &[(&str, RegisterRole, usize)] = {
    use RegisterRole::{Flags, Fp, General, Pc, Sp};
    &[
        ("RIP", Pc, 8),
        ("RSP", Sp, 7),
        ("RBP", Fp, 6),
        ("RAX", General, 0),
        ("RBX", General, 1),
        ("RCX", General, 2),
        ("RDX", General, 3),
        ("RSI", General, 4),
        ("RDI", General, 5),
        ("R8", General, 16),
        ("R9", General, 17),
        ("R10", General, 18),
        ("R11", General, 19),
        ("R12", General, 20),
        ("R13", General, 21),
        ("R14", General, 22),
        ("R15", General, 23),
        ("RFLAGS", Flags, 9),
    ]
};

#[cfg(target_arch = "aarch64")]
const REGISTERS: &[(&str, RegisterRole, usize)] = &[];

impl serde::Serialize for RegisterRole {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match self {
            Self::Pc => "pc",
            Self::Sp => "sp",
            Self::Fp => "fp",
            Self::Lr => "lr",
            Self::Flags => "flags",
            Self::General => "general",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn x86_64_sample_keeps_order_and_marks_pc_sp_fp_and_flags() {
        let registers = from_sample(&RegisterSet([0; 24]), &[]);
        let roles: Vec<_> = registers
            .iter()
            .filter(|r| r.role != RegisterRole::General)
            .map(|r| (r.name.as_str(), r.role))
            .collect();
        assert_eq!(
            roles,
            [
                ("RIP", RegisterRole::Pc),
                ("RSP", RegisterRole::Sp),
                ("RBP", RegisterRole::Fp),
                ("RFLAGS", RegisterRole::Flags),
            ]
        );
        assert_eq!(registers.len(), 18);
        assert_eq!(registers[0].name, "RIP");
        assert_eq!(registers[17].name, "RFLAGS");
    }

    #[test]
    fn register_mapping_keeps_fields_and_offset_or_integer() {
        let map = crate::http_server::model::memory::parse_map(
            "1000-2000 rw-p 00000000 08:01 42 /tmp/a [b]",
        )
        .unwrap();
        let register = classify("RAX", RegisterRole::General, 0x1008, &[map]);
        assert_eq!(register.kind, MemoryKind::File);
        assert_eq!(register.offset, Some(8));
        let json = serde_json::to_value(register).unwrap();
        assert_eq!(
            json["mapping"],
            serde_json::json!({"pathname":"/tmp/a [b]","readable":true,"writable":true,"executable":false,"private":true})
        );
        assert_eq!(json["offset"], "0x8");
        assert_eq!(json["role"], "general");
        let integer = classify("RAX", RegisterRole::General, u64::MAX, &[]);
        assert_eq!(integer.kind, MemoryKind::Integer);
        assert!(integer.mapping.is_none() && integer.offset.is_none());
        assert_eq!(
            serde_json::to_value(integer).unwrap()["offset"],
            serde_json::Value::Null
        );
    }
}
