//! Serialized hardware devices, enriched with GPU and RDMA identity.
//!
//! # Interface
//!
//! | Definition | Visibility | Kind / signature |
//! | --- | --- | --- |
//! | [`GpuInfo`] | `pub` | `struct GpuInfo` |
//! | [`RdmaDeviceInfo`] | `pub` | `struct RdmaDeviceInfo` |
//! | [`HardwareDevice`] | `pub` | `struct HardwareDevice` |
//! | [`device`] | `pub` | `fn device(node: &PciNode, proc: &Path) -> HardwareDevice` |

use super::pci::{BarInfo, HardwareKind, HardwareRole, PciNode, PcieLinkInfo, names, text};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GpuInfo {
    /// `nvidia`, `amd`, `intel` or the PCI vendor ID.
    pub vendor: String,
    pub model: Option<String>,
    pub uuid: Option<String>,
    /// The NVIDIA device minor of `/dev/nvidiaN`.
    pub minor: Option<u32>,
    /// DRM nodes such as `card1` and `renderD128`.
    pub drm: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RdmaDeviceInfo {
    pub name: String,
    pub hca_type: Option<String>,
    pub fw_ver: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HardwareDevice {
    /// A PCI function address such as `0000:85:00.0`, or a host bridge such as `pci0000:80`.
    pub address: String,
    pub parent: Option<String>,
    pub role: HardwareRole,
    pub kind: Option<HardwareKind>,
    pub label: String,
    /// The 24-bit class code as `0x030000`.
    pub class: Option<String>,
    pub vendor_id: Option<String>,
    pub device_id: Option<String>,
    pub driver: Option<String>,
    pub numa_node: Option<u32>,
    /// Null for host bridges and runtime-suspended devices.
    pub link: Option<PcieLinkInfo>,
    pub power_state: Option<String>,
    pub runtime_status: Option<String>,
    pub iommu_group: Option<u32>,
    pub bars: Vec<BarInfo>,
    pub gpu: Option<GpuInfo>,
    pub rdma: Vec<RdmaDeviceInfo>,
    /// Network interfaces of the device.
    pub netdevs: Vec<String>,
}

/// `Key: value` lines of `/proc/driver/nvidia/gpus/<address>/information`.
fn nvidia_information(proc: &Path, address: &str) -> Vec<(String, String)> {
    text(
        &proc
            .join("driver/nvidia/gpus")
            .join(address)
            .join("information"),
    )
    .unwrap_or_default()
    .lines()
    .filter_map(|line| {
        let (key, value) = line.split_once(':')?;
        Some((key.trim().to_owned(), value.trim().to_owned()))
    })
    .collect()
}

fn short(address: &str) -> &str {
    address.strip_prefix("0000:").unwrap_or(address)
}

pub fn device(node: &PciNode, proc: &Path) -> HardwareDevice {
    let hex = |value: Option<u32>, width: usize| value.map(|v| format!("0x{v:0width$x}"));
    let gpu = (node.kind == Some(HardwareKind::Gpu)).then(|| {
        let info = nvidia_information(proc, &node.address);
        let field = |key: &str| info.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
        GpuInfo {
            vendor: match node.vendor_id {
                Some(0x10de) => "nvidia".into(),
                Some(0x1002) => "amd".into(),
                Some(0x8086) => "intel".into(),
                other => hex(other, 4).unwrap_or_default(),
            },
            model: field("Model"),
            uuid: field("GPU UUID"),
            minor: field("Device Minor").and_then(|m| m.parse().ok()),
            drm: names(&node.path.join("drm"))
                .into_iter()
                .filter(|n| n.starts_with("card") || n.starts_with("renderD"))
                .filter(|n| !n.contains('-'))
                .collect(),
        }
    });
    let rdma: Vec<_> = names(&node.path.join("infiniband"))
        .into_iter()
        .map(|name| {
            let dir = node.path.join("infiniband").join(&name);
            RdmaDeviceInfo {
                hca_type: text(&dir.join("hca_type")),
                fw_ver: text(&dir.join("fw_ver")),
                name,
            }
        })
        .collect();
    let label = match (node.role, node.kind) {
        (HardwareRole::HostBridge, _) => {
            format!("Host bridge {}", node.address.trim_start_matches("pci"))
        }
        (HardwareRole::RootPort, _) => format!("Root port {}", short(&node.address)),
        (HardwareRole::SwitchPort, _) => format!("Switch port {}", short(&node.address)),
        (_, Some(HardwareKind::Gpu)) => gpu
            .as_ref()
            .and_then(|g| g.model.clone())
            .unwrap_or_else(|| format!("GPU {}", short(&node.address))),
        (_, Some(HardwareKind::Rdma)) if !rdma.is_empty() => rdma
            .iter()
            .map(|d| d.name.as_str())
            .collect::<Vec<_>>()
            .join(", "),
        (_, Some(HardwareKind::Rdma)) => format!("RDMA NIC {}", short(&node.address)),
        (_, Some(HardwareKind::Accelerator)) => format!("Accelerator {}", short(&node.address)),
        (_, Some(HardwareKind::NvSwitch)) => format!("NVSwitch {}", short(&node.address)),
        (_, None) => short(&node.address).to_owned(),
    };
    HardwareDevice {
        address: node.address.clone(),
        parent: node.parent.clone(),
        role: node.role,
        kind: node.kind,
        label,
        class: hex(node.class, 6),
        vendor_id: hex(node.vendor_id, 4),
        device_id: hex(node.device_id, 4),
        driver: node.driver.clone(),
        numa_node: node.numa_node,
        link: node.link,
        power_state: node.power_state.clone(),
        runtime_status: node.runtime_status.clone(),
        iommu_group: node.iommu_group,
        bars: node.bars.clone(),
        gpu,
        rdma,
        netdevs: if node.role == HardwareRole::Endpoint {
            names(&node.path.join("net"))
        } else {
            vec![]
        },
    }
}

crate::ipc::codec::wire_struct!(GpuInfo {
    vendor,
    model,
    uuid,
    minor,
    drm
});
crate::ipc::codec::wire_struct!(RdmaDeviceInfo {
    name,
    hca_type,
    fw_ver
});
crate::ipc::codec::wire_struct!(HardwareDevice {
    address,
    parent,
    role,
    kind,
    label,
    class,
    vendor_id,
    device_id,
    driver,
    numa_node,
    link,
    power_state,
    runtime_status,
    iommu_group,
    bars,
    gpu,
    rdma,
    netdevs
});
