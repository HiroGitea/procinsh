//! Register values shared with native acquisition.
//!
//! # Interface
//! - [`RegisterSet`] (`pub`): `struct RegisterSet`.
//! - [`RegisterRole`] (`pub`): `enum RegisterRole`.
//! - [`Register`] (`pub`): `struct Register`.
//! - [`RegisterMapping`] (`pub`): `struct RegisterMapping`.
use crate::ipc::model::memory::MemoryKind;

#[derive(Clone, Debug)]
pub struct RegisterSet(pub [u64; 24]);

/// Architecture-independent register purpose, separate from its memory mapping kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegisterRole {
    Pc,
    Sp,
    Fp,
    Lr,
    Flags,
    General,
}
crate::ipc::codec::wire_enum!(RegisterRole{0=>Pc,1=>Sp,2=>Fp,3=>Lr,4=>Flags,5=>General});

#[derive(Clone, Debug)]
pub struct Register {
    pub name: String,
    pub role: RegisterRole,
    pub value: u64,
    pub decimal: String,
    pub kind: MemoryKind,
    pub mapping: Option<RegisterMapping>,
    pub offset: Option<u64>,
}

#[derive(Clone, Debug)]
pub struct RegisterMapping {
    pub pathname: Option<String>,
    pub readable: bool,
    pub writable: bool,
    pub executable: bool,
    pub private: bool,
}

impl crate::ipc::codec::Wire for RegisterSet {
    fn write(&self, o: &mut crate::ipc::codec::Encoder) -> anyhow::Result<()> {
        self.0.write(o)
    }
    fn read(i: &mut crate::ipc::codec::Decoder<'_>) -> anyhow::Result<Self> {
        Ok(Self(crate::ipc::codec::Wire::read(i)?))
    }
}

crate::ipc::codec::wire_struct!(Register {
    name,
    role,
    value,
    decimal,
    kind,
    mapping,
    offset
});

crate::ipc::codec::wire_struct!(RegisterMapping {
    pathname,
    readable,
    writable,
    executable,
    private
});
