//! IPoIB neighbor resolution of IP addresses to remote port GIDs.
//!
//! An IPoIB hardware address is 20 bytes: flags and the remote IPoIB QPN, then the
//! remote port GID. `/proc/net/arp` truncates it, so neighbors are dumped with
//! rtnetlink in the collector's network namespace.

use super::codes::IbGid;
use crate::http_server::netlink::{self, NetlinkDumpRequest};
use anyhow::Result;
use std::{collections::HashMap, net::IpAddr, time::Instant};

const RTM_NEWNEIGH: u16 = 28;
const RTM_GETNEIGH: u16 = 30;
const NDA_DST: u16 = 1;
const NDA_LLADDR: u16 = 2;
const NUD_INCOMPLETE: u16 = 0x01;
const NUD_FAILED: u16 = 0x20;
const IPOIB_HARDWARE_ADDRESS: usize = 20;

/// One `RTM_NEWNEIGH` body; only resolved IPoIB entries yield a GID.
fn parse_neighbor(body: &[u8]) -> Result<Option<(IpAddr, IbGid)>> {
    anyhow::ensure!(body.len() >= 12, "short neighbor message");
    let state = u16::from_ne_bytes(body[8..10].try_into()?);
    if state & (NUD_INCOMPLETE | NUD_FAILED) != 0 {
        return Ok(None);
    }
    let (mut destination, mut hardware) = (None, None);
    for (kind, data) in netlink::attributes(&body[12..])? {
        match kind {
            NDA_DST => {
                destination = match data.len() {
                    4 => Some(IpAddr::from(<[u8; 4]>::try_from(data)?)),
                    16 => Some(IpAddr::from(<[u8; 16]>::try_from(data)?)),
                    _ => None,
                }
            }
            NDA_LLADDR if data.len() == IPOIB_HARDWARE_ADDRESS => {
                hardware = IbGid::from_slice(&data[4..])
            }
            _ => {}
        }
    }
    Ok(destination.zip(hardware))
}

/// Resolved IPoIB neighbors. An address seen with different GIDs is ambiguous and omitted.
pub(super) fn ipoib_gids(deadline: Instant) -> Result<HashMap<IpAddr, IbGid>> {
    let fd = netlink::open_socket(libc::NETLINK_ROUTE, "NETLINK_ROUTE")?;
    let mut found: HashMap<IpAddr, Option<IbGid>> = HashMap::new();
    // struct ndmsg with AF_UNSPEC dumps every neighbor table.
    NetlinkDumpRequest {
        label: "IPoIB neighbor dump",
        message_type: RTM_GETNEIGH,
        reply_type: RTM_NEWNEIGH,
        sequence: 1,
        payload: &[0u8; 12],
    }
    .run(&fd, deadline, |body| {
        if let Some((ip, gid)) = parse_neighbor(body)? {
            let ip = match ip {
                IpAddr::V6(v6) => v6.to_ipv4_mapped().map(IpAddr::V4).unwrap_or(ip),
                v4 => v4,
            };
            found
                .entry(ip)
                .and_modify(|known| {
                    if *known != Some(gid) {
                        *known = None;
                    }
                })
                .or_insert(Some(gid));
        }
        Ok(())
    })?;
    Ok(found
        .into_iter()
        .filter_map(|(ip, gid)| Some((ip, gid?)))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attribute(kind: u16, data: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&((4 + data.len()) as u16).to_ne_bytes());
        bytes.extend_from_slice(&kind.to_ne_bytes());
        bytes.extend_from_slice(data);
        bytes.resize((bytes.len() + 3) & !3, 0);
        bytes
    }

    fn neighbor(state: u16, destination: &[u8], hardware: &[u8]) -> Vec<u8> {
        let mut body = vec![0u8; 12];
        body[8..10].copy_from_slice(&state.to_ne_bytes());
        body.extend(attribute(NDA_DST, destination));
        body.extend(attribute(NDA_LLADDR, hardware));
        body
    }

    #[test]
    fn ipoib_hardware_addresses_yield_remote_gids() {
        let hardware = [
            0x80, 0x00, 0x02, 0x22, 0xfe, 0x80, 0, 0, 0, 0, 0, 0, 0xf4, 0x52, 0x14, 0x03, 0x00,
            0x80, 0xde, 0x51,
        ];
        let (ip, gid) = parse_neighbor(&neighbor(0x04, &[10, 10, 10, 1], &hardware))
            .unwrap()
            .unwrap();
        assert_eq!(ip.to_string(), "10.10.10.1");
        assert_eq!(gid.to_string(), "fe80:0000:0000:0000:f452:1403:0080:de51");
        // Ethernet neighbors and unresolved entries carry no GID.
        assert!(
            parse_neighbor(&neighbor(0x02, &[192, 168, 2, 1], &[0xde; 6]))
                .unwrap()
                .is_none()
        );
        assert!(
            parse_neighbor(&neighbor(NUD_FAILED, &[10, 10, 10, 1], &hardware))
                .unwrap()
                .is_none()
        );
        assert!(parse_neighbor(&[0; 4]).is_err());
    }
}
