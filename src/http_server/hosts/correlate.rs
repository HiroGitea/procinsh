//! Connections whose two ends are observed on different hosts.
//!
//! Each host reports a connection to another host as an external endpoint. A TCP
//! or UDP pair matches by reversed address tuples. Only unique matches link, so
//! NAT and duplicated tuples stay external.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashMap, net::IpAddr};

/// One end of a cross-host link: a host and a relation ID in its snapshot.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub(in crate::http_server) struct LinkEnd {
    pub(in crate::http_server) host: String,
    pub(in crate::http_server) relation: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(in crate::http_server) struct SocketLink {
    pub(in crate::http_server) a: LinkEnd,
    pub(in crate::http_server) b: LinkEnd,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub(in crate::http_server) struct CrossHostLinks {
    pub(in crate::http_server) sockets: Vec<SocketLink>,
}

#[derive(Deserialize)]
struct Kind {
    kind: String,
}
#[derive(Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
struct Address {
    ip: IpAddr,
    port: u16,
}
#[derive(Deserialize)]
struct Socket {
    protocol: Kind,
    local: Option<Address>,
    remote: Option<Address>,
    network_peer: bool,
}
#[derive(Deserialize)]
struct FdRelation {
    id: String,
    peer: Option<Value>,
    shared: bool,
    socket: Option<Socket>,
}

fn entries<T: for<'de> Deserialize<'de>>(snapshot: &Value, name: &str) -> Vec<T> {
    // Entries that do not fit the expected shape are skipped, not fatal.
    snapshot
        .get(name)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| T::deserialize(entry).ok())
        .collect()
}

fn routable(address: Address) -> bool {
    address.port != 0 && !address.ip.is_unspecified() && !address.ip.is_loopback()
}

fn socket_links(hosts: &[(&str, &Value)]) -> Vec<SocketLink> {
    type Tuple = (String, Address, Address);
    let mut by_tuple: HashMap<Tuple, Vec<LinkEnd>> = HashMap::new();
    for (host, snapshot) in hosts {
        for relation in entries::<FdRelation>(snapshot, "fd_relations") {
            let Some(socket) = relation.socket else {
                continue;
            };
            if relation.peer.is_some() || relation.shared || !socket.network_peer {
                continue;
            }
            let (Some(local), Some(remote)) = (socket.local, socket.remote) else {
                continue;
            };
            if !routable(local) || !routable(remote) {
                continue;
            }
            by_tuple
                .entry((socket.protocol.kind, local, remote))
                .or_default()
                .push(LinkEnd {
                    host: (*host).to_owned(),
                    relation: relation.id,
                });
        }
    }
    let mut links = Vec::new();
    for ((kind, local, remote), ends) in &by_tuple {
        let Some(reverse) = by_tuple.get(&(kind.clone(), *remote, *local)) else {
            continue;
        };
        // Each tuple once, from its smaller end; both sides must be unique.
        if ends.len() != 1 || reverse.len() != 1 || ends[0] >= reverse[0] {
            continue;
        }
        if ends[0].host != reverse[0].host {
            links.push(SocketLink {
                a: ends[0].clone(),
                b: reverse[0].clone(),
            });
        }
    }
    links.sort_by(|x, y| (&x.a, &x.b).cmp(&(&y.a, &y.b)));
    links
}

/// Links between `hosts`, given as host names and their snapshots.
pub(in crate::http_server) fn correlate(hosts: &[(&str, &Value)]) -> CrossHostLinks {
    CrossHostLinks {
        sockets: socket_links(hosts),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tcp(id: &str, local: &str, remote: &str) -> Value {
        let address = |text: &str| {
            let a: std::net::SocketAddr = text.parse().unwrap();
            json!({"ip": a.ip(), "port": a.port()})
        };
        json!({"id": id, "peer": null, "shared": false, "socket": {
            "protocol": {"kind": "tcp", "family": "ipv4"}, "state": {"kind": "established"},
            "local": address(local), "remote": address(remote), "network_peer": true}})
    }

    fn host(name: &str, fd: Vec<Value>) -> (&str, Value) {
        (name, json!({"fd_relations": fd}))
    }

    fn correlate(hosts: &[(&str, Value)]) -> CrossHostLinks {
        let hosts: Vec<_> = hosts.iter().map(|(name, s)| (*name, s)).collect();
        super::correlate(&hosts)
    }

    #[test]
    fn reversed_unique_tuples_link_sockets_across_hosts() {
        let links = correlate(&[
            host(
                "a",
                vec![
                    tcp("x", "10.0.0.1:5000", "10.0.0.2:443"),
                    tcp("dup1", "10.0.0.1:1", "10.0.0.9:9"),
                ],
            ),
            host(
                "b",
                vec![
                    tcp("y", "10.0.0.2:443", "10.0.0.1:5000"),
                    tcp("dup2", "10.0.0.9:9", "10.0.0.1:1"),
                    tcp("lo", "127.0.0.1:80", "127.0.0.1:81"),
                ],
            ),
            host(
                "c",
                vec![
                    tcp("dup3", "10.0.0.9:9", "10.0.0.1:1"),
                    tcp("lo2", "127.0.0.1:81", "127.0.0.1:80"),
                ],
            ),
        ]);
        assert_eq!(
            serde_json::to_value(&links.sockets).unwrap(),
            json!([{"a": {"host": "a", "relation": "x"}, "b": {"host": "b", "relation": "y"}}])
        );
        // The same host never links to itself, even with reversed tuples.
        let single = correlate(&[host(
            "a",
            vec![
                tcp("x", "10.0.0.1:1", "10.0.0.2:2"),
                tcp("y", "10.0.0.2:2", "10.0.0.1:1"),
            ],
        )]);
        assert!(single.sockets.is_empty());
        // Malformed entries are ignored.
        let malformed = correlate(&[host("a", vec![json!({"id": 1}), json!("x")])]);
        assert_eq!(malformed, CrossHostLinks::default());
    }
}
