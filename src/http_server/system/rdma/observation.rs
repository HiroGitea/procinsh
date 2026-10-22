//! One RDMA observation for a system snapshot.

use super::{neighbor, nldev, ports, relations};
use super::{ports::RdmaPortInfo, relations::RdmaRelation};
use crate::http_server::process::ProcessId;
use std::{collections::HashMap, path::Path, time::Instant};

const SYSFS: &str = "/sys/class/infiniband";

#[derive(Default)]
pub(in crate::http_server::system) struct RdmaObservation {
    pub(in crate::http_server::system) ports: Vec<RdmaPortInfo>,
    pub(in crate::http_server::system) relations: Vec<RdmaRelation>,
    pub(in crate::http_server::system) warnings: Vec<String>,
}

/// Observes RDMA resources visible in the collector's network and PID namespaces.
/// Failures become warnings; an IPoIB neighbor failure only omits remote GIDs.
pub(in crate::http_server::system) fn observe(
    processes: &HashMap<i32, ProcessId>,
    deadline: Instant,
) -> RdmaObservation {
    let root = Path::new(SYSFS);
    let mut result = RdmaObservation::default();
    // Opening NETLINK_RDMA can autoload ib_core; a host without devices is not queried.
    if ports::devices(root).is_empty() {
        return result;
    }
    result.ports = ports::read_ports(root);
    let resources = match nldev::read_resources(deadline) {
        Ok(resources) => resources,
        Err(error) => {
            result.warnings.push(format!("RDMA: {error:#}"));
            return result;
        }
    };
    for error in &resources.errors {
        result.warnings.push(format!("RDMA: {error}"));
    }
    if resources.truncated {
        result.warnings.push(format!(
            "RDMA queue pair limit of {} reached. Some queue pairs are missing.",
            nldev::QUEUE_PAIR_LIMIT
        ));
    }
    let mut neighbors = HashMap::new();
    if resources
        .cm_ids
        .iter()
        .any(|cm| cm.destination.is_some_and(|d| d.ip().is_some()))
    {
        match neighbor::ipoib_gids(deadline) {
            Ok(found) => neighbors = found,
            Err(error) => result
                .warnings
                .push(format!("RDMA remote GIDs unavailable: {error:#}")),
        }
    }
    result.relations = relations::relations(&resources, &result.ports, &neighbors, processes);
    result
}
