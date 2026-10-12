//! Read socket kinds without identifying or communicating with a peer.
//!
//! # Interface
//!
//! | Definition | Visibility | Kind / signature |
//! | --- | --- | --- |
//! | [`SocketMetadataReader`] | `pub(in crate::http_server)` | `struct SocketMetadataReader` |
//! | [`additional_sockets`] | `pub(in crate::http_server)` | `fn additional_sockets(pid: `[`i32`]`) -> `[`HashMap`]`<`[`u64`]`, `[`SocketInfo`]`>` |

use super::{
    ProcessId, check_identity,
    sockets::{self, SocketInfo},
};
use crate::http_server::socket_types::{SocketProtocol, SocketType};
use anyhow::{Result, bail, ensure};
use std::{
    collections::HashMap,
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
};

/// Lazily owns one pidfd for a process during a descriptor scan.
///
/// Reads only SO_DOMAIN, SO_TYPE and SO_PROTOCOL from temporary FD duplicates;
/// no socket data or pending errors are consumed. Process identity and socket
/// device/inode are checked before publishing a kind. No result is cached
/// across scans, and process-wide permission/unsupported errors suppress
/// repeated attempts for this reader. All descriptors close through RAII.
pub(in crate::http_server) struct SocketMetadataReader {
    process_id: ProcessId,
    pidfd: Option<OwnedFd>,
    unavailable: Option<String>,
}

impl SocketMetadataReader {
    pub(in crate::http_server) fn new(process_id: ProcessId) -> Self {
        Self {
            process_id,
            pidfd: None,
            unavailable: None,
        }
    }

    pub(in crate::http_server) fn read(
        &mut self,
        fd: u32,
        device: u64,
        inode: u64,
    ) -> Result<SocketProtocol> {
        if let Some(reason) = &self.unavailable {
            bail!("{reason}");
        }
        check_identity(self.process_id)?;
        if self.pidfd.is_none() {
            let raw = unsafe { libc::syscall(libc::SYS_pidfd_open, self.process_id.pid, 0) };
            if raw < 0 {
                return self.syscall_error();
            }
            self.pidfd = Some(unsafe { OwnedFd::from_raw_fd(raw as i32) });
            check_identity(self.process_id)?;
        }
        let raw = unsafe {
            libc::syscall(
                libc::SYS_pidfd_getfd,
                self.pidfd.as_ref().unwrap().as_raw_fd(),
                fd,
                0,
            )
        };
        if raw < 0 {
            return self.syscall_error();
        }
        let duplicate = unsafe { OwnedFd::from_raw_fd(raw as i32) };
        let mut stat: libc::stat = unsafe { std::mem::zeroed() };
        ensure!(
            unsafe { libc::fstat(duplicate.as_raw_fd(), &mut stat) } == 0,
            "socket metadata stat: {}",
            std::io::Error::last_os_error()
        );
        ensure!(
            stat.st_mode & libc::S_IFMT == libc::S_IFSOCK
                && stat.st_dev == device
                && stat.st_ino == inode,
            "socket FD identity changed"
        );
        let domain = option(&duplicate, libc::SO_DOMAIN)?;
        let socket_type = option(&duplicate, libc::SO_TYPE)?;
        let protocol = option(&duplicate, libc::SO_PROTOCOL)?;
        check_identity(self.process_id)?;
        Ok(SocketProtocol::from_codes(domain, socket_type, protocol))
    }

    fn syscall_error(&mut self) -> Result<SocketProtocol> {
        let error = std::io::Error::last_os_error();
        if matches!(
            error.raw_os_error(),
            Some(libc::EPERM | libc::EACCES | libc::ENOSYS)
        ) {
            self.unavailable = Some(format!("socket kind: {error}"));
        }
        Err(error.into())
    }
}

fn option(fd: &OwnedFd, name: i32) -> Result<u32> {
    let mut value: i32 = 0;
    let mut length = std::mem::size_of_val(&value) as libc::socklen_t;
    ensure!(
        unsafe {
            libc::getsockopt(
                fd.as_raw_fd(),
                libc::SOL_SOCKET,
                name,
                (&mut value as *mut i32).cast(),
                &mut length,
            )
        } == 0,
        "socket metadata option: {}",
        std::io::Error::last_os_error()
    );
    ensure!(
        length as usize == std::mem::size_of_val(&value) && value >= 0,
        "invalid socket metadata option"
    );
    Ok(value as u32)
}

/// Procfs fallback for socket kinds not present in TCP/UDP/UNIX tables.
/// Missing/unsupported tables are ignored. No peer or connection state is
/// inferred from these tables; Netlink does not expose the socket's SO_TYPE.
pub(in crate::http_server) fn additional_sockets(pid: i32) -> HashMap<u64, SocketInfo> {
    let mut result = HashMap::new();
    for name in ["netlink", "raw", "raw6", "packet"] {
        if let Ok(text) = sockets::read_text(&format!("/proc/{pid}/net/{name}")) {
            result.extend(parse_table(&text, name));
        }
    }
    result
}

fn parse_table(text: &str, name: &str) -> HashMap<u64, SocketInfo> {
    text.lines()
        .skip(1)
        .filter_map(|line| {
            let f: Vec<_> = line.split_whitespace().collect();
            let (index, family, socket_type, protocol) = match name {
                "netlink" => (9, libc::AF_NETLINK, None, f.get(1)?.parse().ok()?),
                "raw" | "raw6" => {
                    let (_, protocol) = f.get(1)?.split_once(':')?;
                    (
                        9,
                        if name == "raw" {
                            libc::AF_INET
                        } else {
                            libc::AF_INET6
                        },
                        Some(SocketType::Raw),
                        u32::from_str_radix(protocol, 16).ok()?,
                    )
                }
                "packet" => (
                    8,
                    libc::AF_PACKET,
                    Some(SocketType::from_code(f.get(2)?.parse().ok()?)),
                    u32::from_str_radix(f.get(3)?, 16).ok()?,
                ),
                _ => return None,
            };
            let inode = f.get(index)?.parse().ok()?;
            if inode == 0 {
                return None;
            }
            Some((
                inode,
                SocketInfo {
                    protocol: SocketProtocol::Other {
                        family: family as u32,
                        socket_type,
                        protocol,
                    },
                    state: None,
                    local: None,
                    remote: None,
                    path: None,
                    peer_inode: None,
                },
            ))
        })
        .collect()
}

/// Probe the target's access policy for privileged tests. Ordinary test runs
/// allow permission/unsupported errors; dev_test requires real metadata access.
#[cfg(test)]
pub(in crate::http_server) fn metadata_available(id: ProcessId, fd: i32) -> bool {
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::metadata(format!("/proc/{}/fd/{fd}", id.pid)).unwrap();
    match SocketMetadataReader::new(id).read(fd as u32, meta.dev(), meta.ino()) {
        Ok(_) => true,
        Err(error) => {
            let restricted = error.downcast_ref::<std::io::Error>().is_some_and(|error| {
                matches!(
                    error.raw_os_error(),
                    Some(libc::EPERM | libc::EACCES | libc::ENOSYS)
                )
            });
            assert!(
                restricted
                    && std::env::var("PROCINSH_REQUIRE_SOCKET_METADATA").as_deref() != Ok("1"),
                "socket metadata access is required: {error:#}"
            );
            eprintln!("socket metadata access is unavailable in this test environment: {error}");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http_server::socket_types::AddressFamily;
    use std::{
        fs,
        os::unix::{fs::MetadataExt, net::UnixDatagram},
    };

    fn reader() -> SocketMetadataReader {
        SocketMetadataReader::new(super::super::identity(std::process::id() as i32).unwrap())
    }

    fn read_socket(reader: &mut SocketMetadataReader, fd: i32) -> SocketProtocol {
        let meta = fs::metadata(format!("/proc/self/fd/{fd}")).unwrap();
        reader.read(fd as u32, meta.dev(), meta.ino()).unwrap()
    }

    #[test]
    fn unbound_socket_kinds_and_queued_data_are_preserved() {
        let tcp =
            unsafe { libc::socket(libc::AF_INET6, libc::SOCK_STREAM | libc::SOCK_CLOEXEC, 0) };
        assert!(tcp >= 0);
        let tcp = unsafe { OwnedFd::from_raw_fd(tcp) };
        if !metadata_available(
            super::super::identity(std::process::id() as i32).unwrap(),
            tcp.as_raw_fd(),
        ) {
            return;
        }
        let udp = unsafe { libc::socket(libc::AF_INET, libc::SOCK_DGRAM | libc::SOCK_CLOEXEC, 0) };
        assert!(udp >= 0);
        let udp = unsafe { OwnedFd::from_raw_fd(udp) };
        let netlink =
            unsafe { libc::socket(libc::AF_NETLINK, libc::SOCK_DGRAM | libc::SOCK_CLOEXEC, 15) };
        assert!(netlink >= 0);
        let netlink = unsafe { OwnedFd::from_raw_fd(netlink) };
        let (unix, sender) = UnixDatagram::pair().unwrap();
        sender.send(b"unchanged").unwrap();
        unix.set_read_timeout(Some(std::time::Duration::from_secs(1)))
            .unwrap();
        let mut reader = reader();
        assert_eq!(
            read_socket(&mut reader, tcp.as_raw_fd()),
            SocketProtocol::Tcp {
                family: AddressFamily::Ipv6
            }
        );
        assert_eq!(
            read_socket(&mut reader, udp.as_raw_fd()),
            SocketProtocol::Udp {
                family: AddressFamily::Ipv4
            }
        );
        assert_eq!(
            read_socket(&mut reader, netlink.as_raw_fd()).to_string(),
            "NETLINK KOBJECT_UEVENT"
        );
        assert_eq!(
            read_socket(&mut reader, unix.as_raw_fd()),
            SocketProtocol::Unix {
                socket_type: SocketType::Dgram
            }
        );
        let mut data = [0; 16];
        let length = unix.recv(&mut data).unwrap();
        assert_eq!(&data[..length], b"unchanged");
    }

    #[test]
    fn another_process_socket_kind_is_read_without_attaching() {
        let target = super::super::test_support::Target::new("ipc");
        if !metadata_available(target.id, 61) {
            return;
        }
        let meta = fs::metadata(format!("/proc/{}/fd/61", target.id.pid)).unwrap();
        let protocol = SocketMetadataReader::new(target.id)
            .read(61, meta.dev(), meta.ino())
            .unwrap();
        assert_eq!(
            protocol,
            SocketProtocol::Unix {
                socket_type: SocketType::Stream
            }
        );
        target.assert_detached();
    }

    #[test]
    fn changed_process_and_descriptor_identities_are_rejected() {
        let socket = UnixDatagram::unbound().unwrap();
        if !metadata_available(
            super::super::identity(std::process::id() as i32).unwrap(),
            socket.as_raw_fd(),
        ) {
            return;
        }
        let meta = fs::metadata(format!("/proc/self/fd/{}", socket.as_raw_fd())).unwrap();
        let mut reader = reader();
        assert!(
            reader
                .read(socket.as_raw_fd() as u32, meta.dev(), meta.ino() + 1)
                .is_err()
        );
        assert!(
            reader
                .read(socket.as_raw_fd() as u32, meta.dev() + 1, meta.ino())
                .is_err()
        );
        let old_fd = socket.as_raw_fd();
        drop(socket);
        assert!(reader.read(old_fd as u32, meta.dev(), meta.ino()).is_err());
        reader.process_id.start_time_ticks += 1;
        assert!(reader.read(0, 0, 0).is_err());
        let mut denied = SocketMetadataReader {
            unavailable: Some("permission denied".into()),
            ..reader
        };
        assert!(denied.read(0, 0, 0).is_err());
    }

    #[test]
    fn proc_fallback_classifies_kinds_without_inventing_peers_or_states() {
        let netlink = parse_table(
            "header\n0 15 2107 00000002 0 0 0 2 0 14130\nshort\n0 x 0 0 0 0 0 0 0 99\n",
            "netlink",
        );
        assert_eq!(
            netlink[&14130].protocol.to_string(),
            "NETLINK KOBJECT_UEVENT"
        );
        assert_eq!(netlink[&14130].state, None);
        assert_eq!(netlink[&14130].peer_inode, None);
        let raw = parse_table(
            "header\n0: 00000000000000000000000000000000:003A 0:0 07 0 0 0 0 0 29622\n",
            "raw6",
        );
        assert_eq!(raw[&29622].protocol.to_string(), "RAW6 ICMPV6");
        let packet = parse_table("header\n0 3 2 0806 2 1 0 0 28172\n", "packet");
        assert_eq!(
            packet[&28172].protocol,
            SocketProtocol::Other {
                family: libc::AF_PACKET as u32,
                socket_type: Some(SocketType::Dgram),
                protocol: 0x806
            }
        );
        assert!(parse_table("header\n0 15 0 0 0 0 0 0 0 0\n", "netlink").is_empty());
    }
}
