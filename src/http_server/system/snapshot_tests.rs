use super as snapshot;
use std::{
    io::{BufRead, BufReader},
    process::{Command, Stdio},
};
struct Child(std::process::Child);

#[test]
fn netns_snapshot_resolves_multiple_namespaces_and_cross_namespace_holders() {
    use crate::http_server::{
        process,
        socket_types::{SocketConnectionStatus, SocketState},
    };
    use std::os::{fd::AsRawFd, unix::fs::MetadataExt};
    let first = process::TestNetworkNamespaceTarget::new(true);
    let second = process::TestNetworkNamespaceTarget::new(true);
    if first.is_none() || second.is_none() {
        // Both unavailable and available namespace data are valid. Check the
        // in-namespace baseline rather than silently dropping the test.
        let fixture = process::TestNetworkNamespaceTarget::new(false).unwrap();
        let snapshot = snapshot::collect(&mut Default::default());
        assert!(
            snapshot
                .processes
                .iter()
                .any(|p| p.identity == fixture.target.id)
        );
        return;
    }
    let first = first.unwrap();
    let second = second.unwrap();
    let snapshot = snapshot::collect(&mut Default::default());
    let connects = |relation: &snapshot::FdRelation,
                    a: (process::ProcessId, u32),
                    b: (process::ProcessId, u32)| {
        relation.peer.as_ref().is_some_and(|peer| {
            let endpoint = (relation.endpoint.process_id, relation.endpoint.fd);
            let peer = (peer.process_id, peer.fd);
            (endpoint == a && peer == b) || (endpoint == b && peer == a)
        })
    };
    for fixture in [&first, &second] {
        let relation = snapshot
            .fd_relations
            .iter()
            .find(|relation| {
                !relation.shared
                    && connects(relation, (fixture.target.id, 61), (fixture.peer_id, 61))
            })
            .unwrap_or_else(|| {
                panic!(
                    "missing UNIX connection for {:?}: {:?}",
                    fixture.target.id, snapshot.warnings
                )
            });
        assert!(!relation.candidate);
        assert_eq!(
            relation.endpoint.connection_status,
            Some(SocketConnectionStatus::PeerPresent)
        );
        assert_eq!(
            relation.socket.as_ref().unwrap().state,
            Some(SocketState::Connected)
        );
        assert!(snapshot.fd_relations.iter().any(|relation| {
            relation.shared && connects(relation, (fixture.target.id, 61), (fixture.peer_id, 63))
        }));
        let inode = std::fs::metadata(format!("/proc/self/fd/{}", fixture.inherited.as_raw_fd()))
            .unwrap()
            .ino();
        assert!(snapshot.fd_relations.iter().any(|relation| {
            relation.shared
                && relation.endpoint.resource.inode == inode
                && connects(
                    relation,
                    (
                        process::identity(std::process::id() as i32).unwrap(),
                        fixture.inherited.as_raw_fd() as u32,
                    ),
                    (fixture.target.id, 0),
                )
        }));
    }
}
impl Drop for Child {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn snapshot_and_fd_list_keep_unconnected_socket_kinds() {
    use crate::http_server::{
        process,
        socket_types::{AddressFamily, SocketConnectionStatus, SocketProtocol},
    };
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    let definitions = [
        (
            libc::AF_INET,
            libc::SOCK_STREAM,
            0,
            SocketProtocol::Tcp {
                family: AddressFamily::Ipv4,
            },
        ),
        (
            libc::AF_INET6,
            libc::SOCK_DGRAM,
            0,
            SocketProtocol::Udp {
                family: AddressFamily::Ipv6,
            },
        ),
        (
            libc::AF_NETLINK,
            libc::SOCK_DGRAM,
            15,
            SocketProtocol::from_codes(16, 2, 15),
        ),
        (
            libc::AF_UNIX,
            libc::SOCK_DGRAM,
            0,
            SocketProtocol::from_codes(1, 2, 0),
        ),
    ];
    let sockets: Vec<_> = definitions
        .iter()
        .map(|&(family, kind, protocol, _)| {
            let raw = unsafe { libc::socket(family, kind | libc::SOCK_CLOEXEC, protocol) };
            assert!(raw >= 0, "{}", std::io::Error::last_os_error());
            unsafe { OwnedFd::from_raw_fd(raw) }
        })
        .collect();
    let id = process::identity(std::process::id() as i32).unwrap();
    let metadata_available = process::metadata_available(id, sockets[0].as_raw_fd());
    // A bound Netlink socket stays visible through procfs when duplication is denied.
    let mut address: libc::sockaddr_nl = unsafe { std::mem::zeroed() };
    address.nl_family = libc::AF_NETLINK as u16;
    assert_eq!(
        unsafe {
            libc::bind(
                sockets[2].as_raw_fd(),
                (&address as *const libc::sockaddr_nl).cast(),
                std::mem::size_of_val(&address) as _,
            )
        },
        0
    );
    let snapshot = snapshot::collect(&mut Default::default());
    let fds = serde_json::to_value(process::fds(id).unwrap()).unwrap();
    for (socket, &(_, _, _, protocol)) in sockets.iter().zip(&definitions) {
        let fd = socket.as_raw_fd() as u32;
        let relation = snapshot
            .fd_relations
            .iter()
            .find(|r| r.endpoint.process_id == id && r.endpoint.fd == fd)
            .expect("unconnected socket retains a relation");
        let entry = fds["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["fd"] == fd)
            .unwrap();
        assert!(relation.peer.is_none());
        if !metadata_available && protocol.is_tcp_or_udp() {
            assert!(relation.socket.is_none());
            assert_eq!(relation.label, "Socket · Type unavailable");
            assert!(entry["protocol"].is_null());
            assert!(entry["state"].is_null());
            assert!(entry["peers"].as_array().unwrap().is_empty());
            continue;
        }
        let protocol = if !metadata_available && matches!(protocol, SocketProtocol::Other { .. }) {
            SocketProtocol::Other {
                family: 16,
                socket_type: None,
                protocol: 15,
            }
        } else {
            protocol
        };
        assert!(!relation.socket.as_ref().unwrap().network_peer);
        assert_eq!(relation.socket.as_ref().unwrap().protocol, protocol);
        assert!(relation.label.starts_with(&protocol.to_string()));
        assert_eq!(entry["protocol"], serde_json::to_value(protocol).unwrap());
        let expected = if metadata_available {
            if matches!(protocol, SocketProtocol::Other { family: 16, .. }) {
                SocketConnectionStatus::PeerPresent
            } else {
                SocketConnectionStatus::Unconnected
            }
        } else {
            SocketConnectionStatus::Unknown
        };
        assert_eq!(relation.endpoint.connection_status, Some(expected));
        assert_eq!(
            entry["connection_status"],
            serde_json::to_value(expected).unwrap()
        );
        assert!(entry["peers"].as_array().unwrap().is_empty());
        if protocol.is_tcp_or_udp() {
            assert!(relation.socket.as_ref().unwrap().state.is_none());
            assert!(entry["state"].is_null());
        }
    }
}

#[test]
fn socket_connection_status_is_retained_for_shared_listening_and_unconnected_fds() {
    use crate::http_server::{process, socket_types::SocketConnectionStatus};
    use std::{
        net::{TcpListener, UdpSocket},
        os::fd::AsRawFd,
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let listener_duplicate = listener.try_clone().unwrap();
    let udp = UdpSocket::bind("127.0.0.1:0").unwrap();
    let udp_duplicate = udp.try_clone().unwrap();
    let id = process::identity(std::process::id() as i32).unwrap();
    let metadata_available = process::metadata_available(id, listener.as_raw_fd());
    let snapshot = snapshot::collect(&mut Default::default());
    let fds = serde_json::to_value(process::fds(id).unwrap()).unwrap();
    for (fd, expected) in [
        (listener.as_raw_fd(), SocketConnectionStatus::Listening),
        (udp.as_raw_fd(), SocketConnectionStatus::Unconnected),
    ] {
        let metadata = std::fs::metadata(format!("/proc/self/fd/{fd}")).unwrap();
        use std::os::unix::fs::MetadataExt;
        let relation = snapshot
            .fd_relations
            .iter()
            .find(|r| r.endpoint.process_id == id && r.endpoint.resource.inode == metadata.ino())
            .unwrap();
        if metadata_available {
            assert_eq!(relation.endpoint.connection_status, Some(expected));
        } else {
            assert_eq!(
                relation.endpoint.connection_status,
                Some(SocketConnectionStatus::Unknown)
            );
        }
        assert_eq!(relation.endpoint.fd_count, 2);
    }
    for (fd, expected) in [
        (listener.as_raw_fd(), "listening"),
        (listener_duplicate.as_raw_fd(), "listening"),
        (udp.as_raw_fd(), "unconnected"),
        (udp_duplicate.as_raw_fd(), "unconnected"),
    ] {
        let entry = fds["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["fd"] == fd)
            .unwrap();
        assert_eq!(
            entry["connection_status"],
            if metadata_available {
                expected
            } else {
                "unknown"
            }
        );
    }
}

#[test]
fn snapshot_finds_pipe_and_unix_peers_and_bounds_work() {
    super::super::super::test_support::build_targets();
    let mut child = Child(
        Command::new("tests/targets/bin/ipc")
            .stdout(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let mut line = String::new();
    BufReader::new(child.0.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let parent = child.0.id() as i32;
    let peer_pid: i32 = line.split_whitespace().nth(2).unwrap().parse().unwrap();
    let snapshot = snapshot::collect(&mut Default::default());
    assert!(snapshot.processes.iter().any(|n| n.identity.pid == parent));
    assert_eq!(
        snapshot
            .processes
            .iter()
            .find(|n| n.identity.pid == peer_pid)
            .and_then(|n| n.parent_id)
            .map(|id| id.pid),
        Some(parent)
    );
    assert!(snapshot.fd_relations.iter().any(|e| {
        !e.shared
            && ((e.endpoint.process_id.pid == parent
                && e.peer
                    .as_ref()
                    .is_some_and(|peer| peer.process_id.pid == peer_pid))
                || (e.endpoint.process_id.pid == peer_pid
                    && e.peer
                        .as_ref()
                        .is_some_and(|peer| peer.process_id.pid == parent)))
    }));
    assert!(snapshot.inspected_fds <= 100_000);
    let json = serde_json::to_value(&snapshot).unwrap();
    for process in json["processes"].as_array().unwrap() {
        assert!(process.get("cpu_percent").is_none());
        assert!(process.get("rss_bytes").is_none());
    }
    let relations = json["fd_relations"].as_array().unwrap();
    assert!(!relations.is_empty());
    for relation in relations {
        assert!(relation["endpoint"]["process_id"].is_object());
        assert!(relation.get("peer").is_some());
        assert!(relation.get("a").is_none());
        assert!(relation.get("b").is_none());
    }
}

#[test]
fn network_destination_survives_shared_socket_ownership() {
    let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    socket.connect("127.0.0.1:54321").unwrap();
    let inherited: std::os::fd::OwnedFd = socket.try_clone().unwrap().into();
    let child = Child(
        Command::new("sleep")
            .arg("30")
            .stdin(Stdio::from(inherited))
            .spawn()
            .unwrap(),
    );
    let data = snapshot::collect(&mut Default::default());
    for pid in [std::process::id() as i32, child.0.id() as i32] {
        let relation = data
            .fd_relations
            .iter()
            .find(|e| {
                e.endpoint.process_id.pid == pid
                    && e.peer.is_none()
                    && e.socket
                        .as_ref()
                        .is_some_and(|s| s.remote == Some(socket.peer_addr().unwrap().into()))
            })
            .expect("shared socket retains a network destination for each owner");
        let info = relation.socket.as_ref().unwrap();
        assert!(info.network_peer);
        assert_eq!(info.local, Some(socket.local_addr().unwrap().into()));
        assert_eq!(
            info.protocol,
            crate::http_server::socket_types::SocketProtocol::Udp {
                family: crate::http_server::socket_types::AddressFamily::Ipv4
            }
        );
    }
    assert!(data.fd_relations.iter().any(|e| {
        e.shared
            && e.peer.as_ref().is_some_and(|peer| {
                peer.process_id.pid == child.0.id() as i32
                    || e.endpoint.process_id.pid == child.0.id() as i32
            })
    }));
}
