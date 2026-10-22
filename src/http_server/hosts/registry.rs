//! Hosts known to a hub: the local machine and remote agents by name.

use super::feed::{HostFeed, HostStatus, HostSummary};
use super::identity::{HostInfo, valid_host_name};
use crate::http_server::process::timestamp_ms;
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};
use tokio::sync::watch;

const HOST_LIMIT: usize = 64;
/// Offline hosts without viewers stay listed this long.
const OFFLINE_RETENTION_MS: u64 = 10 * 60 * 1000;

pub(in crate::http_server) struct HostRegistry {
    local: HostInfo,
    started: u64,
    hosts: Mutex<BTreeMap<String, Arc<HostFeed>>>,
    links: AtomicU64,
    /// Increments whenever a host is added, removed, or changes status.
    changes: watch::Sender<u64>,
}

impl HostRegistry {
    pub(in crate::http_server) fn new(local: HostInfo) -> Self {
        Self {
            local,
            started: timestamp_ms(),
            hosts: Mutex::new(BTreeMap::new()),
            links: AtomicU64::new(0),
            changes: watch::channel(0).0,
        }
    }

    pub(in crate::http_server) fn local(&self) -> &HostInfo {
        &self.local
    }

    pub(in crate::http_server) fn get(&self, name: &str) -> Option<Arc<HostFeed>> {
        self.hosts.lock().unwrap().get(name).cloned()
    }

    /// Remote hosts in name order.
    pub(in crate::http_server) fn feeds(&self) -> Vec<(String, Arc<HostFeed>)> {
        self.prune();
        self.hosts
            .lock()
            .unwrap()
            .iter()
            .map(|(name, feed)| (name.clone(), feed.clone()))
            .collect()
    }

    pub(in crate::http_server) fn changes(&self) -> watch::Receiver<u64> {
        self.changes.subscribe()
    }

    /// The local host first, then remote hosts in name order.
    pub(in crate::http_server) fn summaries(
        &self,
        local_captured_at: Option<u64>,
    ) -> Vec<HostSummary> {
        let mut result = vec![HostSummary {
            name: self.local.name.clone(),
            local: true,
            status: HostStatus::Online {
                since: self.started,
            },
            machine_id: self.local.machine_id.clone(),
            version: self.local.version.clone(),
            kernel: self.local.kernel.clone(),
            captured_at: local_captured_at,
        }];
        result.extend(self.feeds().into_iter().map(|(_, feed)| feed.summary()));
        result
    }

    /// Admits an agent under its name and returns its feed and link ID.
    /// A reconnect from the same machine replaces the previous link; an online
    /// host of another machine keeps its name.
    pub(super) fn admit(&self, info: HostInfo) -> Result<(Arc<HostFeed>, u64), String> {
        if !valid_host_name(&info.name) {
            return Err(format!("invalid host name {:?}", info.name));
        }
        if info.name == self.local.name {
            return Err(format!("host name {} is the hub's own name", info.name));
        }
        self.prune();
        let link = self.links.fetch_add(1, Ordering::Relaxed) + 1;
        let mut hosts = self.hosts.lock().unwrap();
        let feed = match hosts.get(&info.name) {
            Some(feed) => {
                if feed.online() && feed.info().machine_id != info.machine_id {
                    return Err(format!(
                        "host name {} is already connected from another machine",
                        info.name
                    ));
                }
                feed.connect(info, link);
                feed.clone()
            }
            None => {
                if hosts.len() >= HOST_LIMIT {
                    return Err(format!("the hub accepts at most {HOST_LIMIT} hosts"));
                }
                let feed = HostFeed::new(info.clone(), link);
                hosts.insert(info.name, feed.clone());
                feed
            }
        };
        drop(hosts);
        self.changes.send_modify(|generation| *generation += 1);
        Ok((feed, link))
    }

    pub(super) fn disconnected(&self, feed: &HostFeed, link: u64) {
        feed.disconnect(link);
        self.changes.send_modify(|generation| *generation += 1);
    }

    fn prune(&self) {
        let now = timestamp_ms();
        let mut hosts = self.hosts.lock().unwrap();
        let before = hosts.len();
        hosts.retain(|_, feed| {
            feed.idle_since()
                .is_none_or(|since| now.saturating_sub(since) < OFFLINE_RETENTION_MS)
        });
        if hosts.len() != before {
            drop(hosts);
            self.changes.send_modify(|generation| *generation += 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(name: &str, machine: &str) -> HostInfo {
        HostInfo {
            name: name.into(),
            machine_id: machine.into(),
            version: "v".into(),
            kernel: None,
        }
    }

    #[test]
    fn names_are_unique_per_machine_and_reconnects_replace_links() {
        let registry = HostRegistry::new(info("hub", "h"));
        let mut changes = registry.changes();
        assert!(registry.admit(info("hub", "x")).is_err());
        assert!(registry.admit(info("all", "x")).is_err());
        let (feed, first) = registry.admit(info("b", "m1")).unwrap();
        assert!(changes.has_changed().unwrap());
        changes.mark_unchanged();
        assert!(
            registry
                .admit(info("b", "m2"))
                .err()
                .unwrap()
                .contains("another machine")
        );
        let (same, second) = registry.admit(info("b", "m1")).unwrap();
        assert!(Arc::ptr_eq(&feed, &same) && first != second);
        registry.disconnected(&feed, first);
        assert!(
            feed.online(),
            "a replaced link does not mark the host offline"
        );
        registry.disconnected(&feed, second);
        assert!(!feed.online());
        // Another machine may take the name of an offline host.
        let (taken, _) = registry.admit(info("b", "m2")).unwrap();
        assert!(Arc::ptr_eq(&feed, &taken));
        let names: Vec<_> = registry
            .summaries(None)
            .into_iter()
            .map(|s| (s.name, s.local))
            .collect();
        assert_eq!(names, [("hub".into(), true), ("b".into(), false)]);
        assert!(registry.get("b").is_some() && registry.get("c").is_none());
        for i in 0..HOST_LIMIT - 1 {
            registry.admit(info(&format!("n{i}"), "m")).unwrap();
        }
        assert!(
            registry
                .admit(info("over", "m"))
                .err()
                .unwrap()
                .contains("at most")
        );
    }
}
