//! Local RDMA port addresses and link state from sysfs.

use super::codes::IbGid;
use serde::Serialize;
use std::{fs, path::Path};

/// One port of an RDMA device. Unassigned LIDs and GIDs are null.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(in crate::http_server::system) struct RdmaPortInfo {
    pub(in crate::http_server::system) device: String,
    pub(in crate::http_server::system) port: u32,
    /// `InfiniBand` or `Ethernet` (RoCE), as written by the kernel.
    pub(in crate::http_server::system) link_layer: Option<String>,
    /// Logical port state, such as `ACTIVE`.
    pub(in crate::http_server::system) state: Option<String>,
    pub(in crate::http_server::system) lid: Option<u32>,
    pub(in crate::http_server::system) gid: Option<IbGid>,
    pub(in crate::http_server::system) rate: Option<String>,
}

fn text(path: &Path) -> Option<String> {
    let value = fs::read_to_string(path).ok()?;
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn names(path: &Path) -> Vec<String> {
    let mut names: Vec<_> = fs::read_dir(path)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// Device names under `root` (`/sys/class/infiniband`).
pub(super) fn devices(root: &Path) -> Vec<String> {
    names(root)
}

pub(super) fn read_ports(root: &Path) -> Vec<RdmaPortInfo> {
    let mut result = Vec::new();
    for device in devices(root) {
        let ports = root.join(&device).join("ports");
        let mut numbers: Vec<u32> = names(&ports)
            .iter()
            .filter_map(|name| name.parse().ok())
            .collect();
        numbers.sort();
        for port in numbers {
            let path = ports.join(port.to_string());
            result.push(RdmaPortInfo {
                device: device.clone(),
                port,
                link_layer: text(&path.join("link_layer")),
                state: text(&path.join("state")).map(|s| {
                    s.split_once(": ")
                        .map_or(s.clone(), |(_, name)| name.into())
                }),
                lid: text(&path.join("lid"))
                    .and_then(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok())
                    .filter(|lid| *lid != 0),
                gid: text(&path.join("gids/0")).and_then(|s| IbGid::parse(&s)),
                rate: text(&path.join("rate")),
            });
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sysfs_ports_parse_addresses_and_keep_unassigned_values_null() {
        let root = std::env::temp_dir().join(format!("procinsh-rdma-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        for (device, port, lid, gid) in [
            (
                "mlx4_0",
                "1",
                "0x2",
                "fe80:0000:0000:0000:0002:c903:0028:45c7",
            ),
            (
                "mlx4_0",
                "2",
                "0x0",
                "0000:0000:0000:0000:0000:0000:0000:0000",
            ),
            (
                "mlx4_0",
                "10",
                "0x10",
                "fe80:0000:0000:0000:0002:c903:0028:45c8",
            ),
        ] {
            let path = root.join(device).join("ports").join(port);
            fs::create_dir_all(path.join("gids")).unwrap();
            fs::write(path.join("lid"), format!("{lid}\n")).unwrap();
            fs::write(path.join("gids/0"), format!("{gid}\n")).unwrap();
            fs::write(path.join("state"), "4: ACTIVE\n").unwrap();
            fs::write(path.join("link_layer"), "InfiniBand\n").unwrap();
        }
        fs::create_dir_all(root.join("rxe0")).unwrap();
        let ports = read_ports(&root);
        assert_eq!(devices(&root), ["mlx4_0", "rxe0"]);
        assert_eq!(ports.iter().map(|p| p.port).collect::<Vec<_>>(), [1, 2, 10]);
        assert_eq!(
            serde_json::to_value(&ports[0]).unwrap(),
            serde_json::json!({
                "device": "mlx4_0",
                "port": 1,
                "link_layer": "InfiniBand",
                "state": "ACTIVE",
                "lid": 2,
                "gid": "fe80:0000:0000:0000:0002:c903:0028:45c7",
                "rate": null,
            })
        );
        assert_eq!((ports[1].lid, ports[1].gid), (None, None));
        fs::remove_dir_all(&root).unwrap();
    }
}
