//! Native-endian binary values for the private, same-machine IPC connection.
//!
//! # Interface
//!
//! | Definition | Visibility | Kind / signature |
//! | --- | --- | --- |
//! | [`Wire`] | `pub` | `trait Wire` |
//! | [`Encoder`] | `pub` | `struct Encoder` |
//! | [`Decoder`] | `pub` | `struct Decoder<'a>` |
//! | [`LimitExceeded`] | `pub` | `struct LimitExceeded` |
//! | [`encode`] | `pub` | `fn encode<T: Wire>(value: &T, limit: usize) -> anyhow::Result<Vec<u8>>` |
//! | [`decode`] | `pub` | `fn decode<T: Wire>(bytes: &[u8]) -> anyhow::Result<T>` |
//!
//! Struct fields follow their declared wire order; enum variants have explicit
//! byte tags. Strings and collections have u32 lengths. Integers and floats use
//! their native byte representation. Rust padding, pointers and container layouts
//! are never copied. The peers must use the same protocol version and architecture.
use anyhow::{Result, bail, ensure};
use serde_json::Value;
use std::{collections::BTreeMap, sync::Arc};

pub(crate) const MAGIC: &[u8; 4] = b"PCI1";

#[derive(Debug)]
pub struct LimitExceeded;
impl std::fmt::Display for LimitExceeded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("monitor response exceeds size limit")
    }
}
impl std::error::Error for LimitExceeded {}

pub struct Encoder {
    bytes: Vec<u8>,
    limit: usize,
}
pub struct Decoder<'a> {
    bytes: &'a [u8],
    depth: usize,
}
/// An owned value with an explicit wire representation independent of Rust layout.
pub trait Wire: Sized {
    fn write(&self, output: &mut Encoder) -> Result<()>;
    fn read(input: &mut Decoder<'_>) -> Result<Self>;
}
impl Encoder {
    pub(crate) fn append(&mut self, bytes: &[u8]) -> Result<()> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            return Err(LimitExceeded.into());
        }
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }
    pub(crate) fn length(&mut self, length: usize) -> Result<()> {
        u32::try_from(length)
            .map_err(|_| LimitExceeded)?
            .write(self)
    }
    pub(crate) fn blob(&mut self, bytes: &[u8]) -> Result<()> {
        self.length(bytes.len())?;
        self.append(bytes)
    }
}
impl<'a> Decoder<'a> {
    pub(crate) fn take(&mut self, length: usize) -> Result<&'a [u8]> {
        ensure!(length <= self.bytes.len(), "truncated monitor value");
        let (bytes, remaining) = self.bytes.split_at(length);
        self.bytes = remaining;
        Ok(bytes)
    }
    pub(crate) fn length(&mut self) -> Result<usize> {
        let length = u32::read(self)? as usize;
        ensure!(
            length <= self.bytes.len(),
            "invalid monitor collection length"
        );
        Ok(length)
    }
    pub(crate) fn blob(&mut self) -> Result<&'a [u8]> {
        let length = self.length()?;
        self.take(length)
    }
    fn nested<T>(&mut self, read: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        ensure!(self.depth < 128, "monitor value nesting limit exceeded");
        self.depth += 1;
        let result = read(self);
        self.depth -= 1;
        result
    }
}
pub fn encode<T: Wire>(value: &T, limit: usize) -> Result<Vec<u8>> {
    let mut output = Encoder {
        bytes: Vec::new(),
        limit: limit.min(u32::MAX as usize),
    };
    output.append(MAGIC)?;
    value.write(&mut output)?;
    Ok(output.bytes)
}
pub fn decode<T: Wire>(bytes: &[u8]) -> Result<T> {
    let mut input = Decoder { bytes, depth: 0 };
    ensure!(
        input.take(MAGIC.len())? == MAGIC,
        "unsupported monitor protocol version"
    );
    let value = T::read(&mut input)?;
    ensure!(input.bytes.is_empty(), "trailing monitor bytes");
    Ok(value)
}
macro_rules! number {
    ($($type:ty),+ $(,)?) => {$(
        impl Wire for $type {
            fn write(&self, output: &mut Encoder) -> Result<()> { output.append(&self.to_ne_bytes()) }
            fn read(input: &mut Decoder<'_>) -> Result<Self> {
                Ok(Self::from_ne_bytes(input.take(std::mem::size_of::<Self>())?.try_into()?))
            }
        }
    )+};
}
number!(u8, u16, u32, u64, i32, i64, f64, usize);
impl Wire for bool {
    fn write(&self, output: &mut Encoder) -> Result<()> {
        u8::from(*self).write(output)
    }
    fn read(input: &mut Decoder<'_>) -> Result<Self> {
        match u8::read(input)? {
            0 => Ok(false),
            1 => Ok(true),
            _ => bail!("invalid monitor boolean"),
        }
    }
}
impl Wire for String {
    fn write(&self, output: &mut Encoder) -> Result<()> {
        output.blob(self.as_bytes())
    }
    fn read(input: &mut Decoder<'_>) -> Result<Self> {
        Ok(std::str::from_utf8(input.blob()?)?.to_owned())
    }
}
impl<T: Wire> Wire for Vec<T> {
    fn write(&self, output: &mut Encoder) -> Result<()> {
        output.length(self.len())?;
        for value in self {
            value.write(output)?;
        }
        Ok(())
    }
    fn read(input: &mut Decoder<'_>) -> Result<Self> {
        let length = input.length()?;
        let mut values = Vec::with_capacity(length.min(4096));
        for _ in 0..length {
            values.push(T::read(input)?);
        }
        Ok(values)
    }
}
impl<T: Wire, const N: usize> Wire for [T; N] {
    fn write(&self, output: &mut Encoder) -> Result<()> {
        for value in self {
            value.write(output)?;
        }
        Ok(())
    }
    fn read(input: &mut Decoder<'_>) -> Result<Self> {
        let mut values = Vec::with_capacity(N);
        for _ in 0..N {
            values.push(T::read(input)?);
        }
        values
            .try_into()
            .map_err(|_| anyhow::anyhow!("invalid monitor array"))
    }
}
impl<T: Wire> Wire for Option<T> {
    fn write(&self, output: &mut Encoder) -> Result<()> {
        match self {
            None => 0u8.write(output),
            Some(value) => {
                1u8.write(output)?;
                value.write(output)
            }
        }
    }
    fn read(input: &mut Decoder<'_>) -> Result<Self> {
        match u8::read(input)? {
            0 => Ok(None),
            1 => Ok(Some(T::read(input)?)),
            _ => bail!("invalid monitor option"),
        }
    }
}
impl<T: Wire, E: Wire> Wire for std::result::Result<T, E> {
    fn write(&self, output: &mut Encoder) -> Result<()> {
        match self {
            Ok(value) => {
                0u8.write(output)?;
                value.write(output)
            }
            Err(error) => {
                1u8.write(output)?;
                error.write(output)
            }
        }
    }
    fn read(input: &mut Decoder<'_>) -> Result<Self> {
        match u8::read(input)? {
            0 => Ok(Ok(T::read(input)?)),
            1 => Ok(Err(E::read(input)?)),
            _ => bail!("invalid monitor result"),
        }
    }
}
impl<T: Wire> Wire for Arc<T> {
    fn write(&self, output: &mut Encoder) -> Result<()> {
        self.as_ref().write(output)
    }
    fn read(input: &mut Decoder<'_>) -> Result<Self> {
        Ok(Arc::new(T::read(input)?))
    }
}
macro_rules! tuple {
    ($($type:ident:$index:tt),+)=>{
        impl<$($type:Wire),+> Wire for ($($type,)+) {
            fn write(&self,output:&mut Encoder)->Result<()> {$(self.$index.write(output)?;)+Ok(())}
            fn read(input:&mut Decoder<'_>)->Result<Self> {Ok(($($type::read(input)?,)+))}
        }
    };
}
tuple!(A:0,B:1);
tuple!(A:0,B:1,C:2);
tuple!(A:0,B:1,C:2,D:3,E:4);
impl Wire for std::net::IpAddr {
    fn write(&self, output: &mut Encoder) -> Result<()> {
        match self {
            Self::V4(ip) => {
                4u8.write(output)?;
                ip.octets().write(output)
            }
            Self::V6(ip) => {
                6u8.write(output)?;
                ip.octets().write(output)
            }
        }
    }
    fn read(input: &mut Decoder<'_>) -> Result<Self> {
        match u8::read(input)? {
            4 => Ok(Self::V4(<[u8; 4]>::read(input)?.into())),
            6 => Ok(Self::V6(<[u8; 16]>::read(input)?.into())),
            _ => bail!("invalid monitor IP family"),
        }
    }
}
impl Wire for std::net::SocketAddr {
    fn write(&self, output: &mut Encoder) -> Result<()> {
        self.ip().write(output)?;
        self.port().write(output)?;
        if let Self::V6(address) = self {
            address.flowinfo().write(output)?;
            address.scope_id().write(output)?;
        }
        Ok(())
    }
    fn read(input: &mut Decoder<'_>) -> Result<Self> {
        let ip = std::net::IpAddr::read(input)?;
        let port = u16::read(input)?;
        Ok(match ip {
            std::net::IpAddr::V4(ip) => Self::V4(std::net::SocketAddrV4::new(ip, port)),
            std::net::IpAddr::V6(ip) => Self::V6(std::net::SocketAddrV6::new(
                ip,
                port,
                u32::read(input)?,
                u32::read(input)?,
            )),
        })
    }
}

impl Wire for Value {
    fn write(&self, output: &mut Encoder) -> Result<()> {
        match self {
            Self::Null => 0u8.write(output),
            Self::Bool(false) => 1u8.write(output),
            Self::Bool(true) => 2u8.write(output),
            Self::Number(value) => {
                if let Some(value) = value.as_u64() {
                    3u8.write(output)?;
                    value.write(output)
                } else if let Some(value) = value.as_i64() {
                    4u8.write(output)?;
                    value.write(output)
                } else {
                    5u8.write(output)?;
                    value
                        .as_f64()
                        .ok_or_else(|| anyhow::anyhow!("invalid monitor number"))?
                        .write(output)
                }
            }
            Self::String(value) => {
                6u8.write(output)?;
                value.write(output)
            }
            Self::Array(values) => {
                if values
                    .iter()
                    .all(|value| value.as_u64().is_some_and(|value| value <= 255))
                {
                    9u8.write(output)?;
                    output.length(values.len())?;
                    for value in values {
                        output.append(&[value.as_u64().unwrap() as u8])?;
                    }
                    Ok(())
                } else {
                    7u8.write(output)?;
                    values.write(output)
                }
            }
            Self::Object(values) => {
                8u8.write(output)?;
                output.length(values.len())?;
                for (key, value) in values {
                    key.write(output)?;
                    value.write(output)?;
                }
                Ok(())
            }
        }
    }
    fn read(input: &mut Decoder<'_>) -> Result<Self> {
        input.nested(|input| {
            Ok(match u8::read(input)? {
                0 => Self::Null,
                1 => Self::Bool(false),
                2 => Self::Bool(true),
                3 => Self::from(u64::read(input)?),
                4 => Self::from(i64::read(input)?),
                5 => Self::Number(
                    serde_json::Number::from_f64(f64::read(input)?)
                        .ok_or_else(|| anyhow::anyhow!("invalid monitor float"))?,
                ),
                6 => Self::String(String::read(input)?),
                7 => Self::Array(Vec::read(input)?),
                8 => {
                    let length = input.length()?;
                    let mut values = BTreeMap::new();
                    for _ in 0..length {
                        let key = String::read(input)?;
                        ensure!(
                            values.insert(key, Self::read(input)?).is_none(),
                            "duplicate monitor field"
                        );
                    }
                    Self::Object(values.into_iter().collect())
                }
                9 => Self::Array(input.blob()?.iter().copied().map(Self::from).collect()),
                _ => bail!("invalid monitor value tag"),
            })
        })
    }
}
macro_rules! wire_struct {
    ($type:ty { $($field:ident),* $(,)? })=>{
        impl $crate::ipc::codec::Wire for $type {
            fn write(&self,output:&mut $crate::ipc::codec::Encoder)->anyhow::Result<()> {$( $crate::ipc::codec::Wire::write(&self.$field,output)?; )*Ok(())}
            fn read(input:&mut $crate::ipc::codec::Decoder<'_>)->anyhow::Result<Self> {Ok(Self {$($field:$crate::ipc::codec::Wire::read(input)?,)*})}
        }
    };
}
macro_rules! wire_enum {
    ($type:ty { $($tag:literal => $variant:ident $(($($tuple:ident),+))? $({$($field:ident),*})? ),* $(,)? })=>{
        impl $crate::ipc::codec::Wire for $type {
            fn write(&self,output:&mut $crate::ipc::codec::Encoder)->anyhow::Result<()> {
                match self {$(Self::$variant $(($($tuple),+))? $({$($field),*})? => {
                    $crate::ipc::codec::Wire::write(&($tag as u8),output)?;
                    $($( $crate::ipc::codec::Wire::write($tuple,output)?; )+)?
                    $($( $crate::ipc::codec::Wire::write($field,output)?; )*)?
                    Ok(())
                }),*}
            }
            fn read(input:&mut $crate::ipc::codec::Decoder<'_>)->anyhow::Result<Self> {
                match <u8 as $crate::ipc::codec::Wire>::read(input)? {
                    $($tag=>Ok(Self::$variant $(($({let $tuple=$crate::ipc::codec::Wire::read(input)?;$tuple}),+))? $({$($field:$crate::ipc::codec::Wire::read(input)?),*})?),)*
                    _=>anyhow::bail!("invalid monitor enum tag"),
                }
            }
        }
    };
}
pub(crate) use {wire_enum, wire_struct};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::schema::*;
    use serde_json::json;
    #[test]
    fn native_fields_preserve_numeric_values_strings_and_socket_scope() {
        let value = json!({"unsigned":u64::MAX,"signed":i64::MIN,"float":1.25,"bytes":[0,1,255],"empty":[],"text":"日本語\0literal","nested":{"value":null,"flag":true}});
        let bytes = encode(&value, 4096).unwrap();
        assert_eq!(decode::<Value>(&bytes).unwrap(), value);
        let scalar = 0x0102030405060708u64;
        let number = encode(&scalar, 128).unwrap();
        assert_eq!(&number[4..], &scalar.to_ne_bytes());
        let address = std::net::SocketAddr::V6(std::net::SocketAddrV6::new(
            "fe80::1".parse().unwrap(),
            9090,
            123,
            7,
        ));
        assert_eq!(
            decode::<std::net::SocketAddr>(&encode(&address, 128).unwrap()).unwrap(),
            address
        );
    }
    #[test]
    fn closed_requests_reject_truncation_unknown_tags_and_trailing_fields() {
        let request = MonitorRequest::MappedElf {
            id: ProcessId {
                pid: 42,
                start_time_ticks: u64::MAX,
            },
            mapping_start: 0x1234,
            known_file: Some((1, 2, 3, 4, 5)),
        };
        let bytes = encode(&request, MAX_REQUEST).unwrap();
        assert!(matches!(
            decode::<MonitorRequest>(&bytes).unwrap(),
            MonitorRequest::MappedElf {
                id: ProcessId {
                    pid: 42,
                    start_time_ticks: u64::MAX
                },
                mapping_start: 0x1234,
                known_file: Some((1, 2, 3, 4, 5))
            }
        ));
        for length in 0..bytes.len() {
            assert!(decode::<MonitorRequest>(&bytes[..length]).is_err());
        }
        let mut extra = bytes.clone();
        extra.push(0);
        assert!(decode::<MonitorRequest>(&extra).is_err());
        let mut unknown = MAGIC.to_vec();
        unknown.push(255);
        assert!(decode::<MonitorRequest>(&unknown).is_err());
        let mut invalid_option = bytes;
        invalid_option[4 + 1 + 4 + 8 + 8] = 2;
        assert!(decode::<MonitorRequest>(&invalid_option).is_err());
    }
    #[test]
    fn malformed_lengths_utf8_booleans_depth_and_output_limits_are_rejected() {
        let mut length = MAGIC.to_vec();
        length.extend_from_slice(&u32::MAX.to_ne_bytes());
        assert!(decode::<Vec<u64>>(&length).is_err());
        let mut text = MAGIC.to_vec();
        text.extend_from_slice(&1u32.to_ne_bytes());
        text.push(255);
        assert!(decode::<String>(&text).is_err());
        let mut boolean = MAGIC.to_vec();
        boolean.push(2);
        assert!(decode::<bool>(&boolean).is_err());
        let mut deep = MAGIC.to_vec();
        for _ in 0..129 {
            deep.push(7);
            deep.extend_from_slice(&1u32.to_ne_bytes());
        }
        deep.push(0);
        assert!(decode::<Value>(&deep).is_err());
        assert!(
            encode(&"too large".to_string(), 8)
                .unwrap_err()
                .is::<LimitExceeded>()
        );
    }
    #[test]
    fn captured_snapshots_remain_typed_until_main_processing() {
        let snapshot = crate::model::system::SystemSnapshot {
            captured_at: 123,
            raw_sockets: vec![(
                10,
                crate::model::sockets::SocketInfo {
                    protocol: crate::model::socket_types::SocketProtocol::Tcp {
                        family: crate::model::socket_types::AddressFamily::Ipv6,
                    },
                    state: Some(crate::model::socket_types::SocketState::UnknownInet(99)),
                    local: Some("[::1]:1234".parse().unwrap()),
                    remote: None,
                    path: None,
                    peer_inode: None,
                },
            )],
            ..Default::default()
        };
        let event = MonitorEvent::AcquiredSnapshot(Arc::new(snapshot));
        let bytes = encode(&event, MAX_RESPONSE).unwrap();
        let MonitorEvent::AcquiredSnapshot(decoded) = decode::<MonitorEvent>(&bytes).unwrap()
        else {
            panic!()
        };
        assert_eq!(decoded.captured_at, 123);
        assert_eq!(decoded.raw_sockets[0].0, 10);
        assert_eq!(
            decoded.raw_sockets[0].1.state,
            Some(crate::model::socket_types::SocketState::UnknownInet(99))
        );
    }
}
