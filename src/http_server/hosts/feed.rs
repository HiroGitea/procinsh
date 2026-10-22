//! One remote host's relayed snapshots, activity, link status and viewers.

use super::identity::HostInfo;
use super::merge::SnapshotBaseline;
use crate::http_server::process::timestamp_ms;
use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::Value;
use std::sync::{Arc, Mutex};
use tokio::sync::{broadcast, watch};

/// Browsers per host, matching the local system stream.
const VIEWER_LIMIT: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub(in crate::http_server) enum HostStatus {
    /// `since` is the Unix time in milliseconds of the transition.
    Online {
        since: u64,
    },
    Offline {
        since: u64,
    },
}

#[derive(Clone, Debug)]
pub(in crate::http_server) enum FeedEvent {
    /// A browser-ready snapshot payload with the hub's per-host sequence.
    Snapshot {
        sequence: u64,
        data: Arc<str>,
    },
    Activity(Arc<str>),
    Status(HostStatus),
}

#[derive(Debug, PartialEq, Eq)]
pub(in crate::http_server) enum FeedSubscribeError {
    TooManySubscribers,
}

struct FeedState {
    info: HostInfo,
    status: HostStatus,
    /// The current agent link; frames from replaced links are ignored.
    link: u64,
    baseline: SnapshotBaseline,
    sequence: u64,
    captured_at: Option<u64>,
    viewers: usize,
}

/// Relays one agent's observations to browsers. The feed outlives agent links,
/// so viewers keep their subscription while the agent reconnects.
pub(in crate::http_server) struct HostFeed {
    state: Mutex<FeedState>,
    events: broadcast::Sender<FeedEvent>,
    watched: watch::Sender<bool>,
}

/// A browser's registration; dropping it releases the viewer.
pub(in crate::http_server) struct FeedSubscription {
    feed: Arc<HostFeed>,
    pub(in crate::http_server) receiver: broadcast::Receiver<FeedEvent>,
    /// Full snapshot JSON of the merged state when subscribing.
    pub(in crate::http_server) initial: String,
    pub(in crate::http_server) initial_sequence: u64,
    pub(in crate::http_server) status: HostStatus,
}
impl Drop for FeedSubscription {
    fn drop(&mut self) {
        let mut state = self.feed.state.lock().unwrap();
        state.viewers -= 1;
        if state.viewers == 0 {
            // The agent stops collecting; a stale state must not greet the next viewer.
            state.baseline.clear();
            state.captured_at = None;
            self.feed.watched.send_replace(false);
        }
    }
}

/// A host list entry.
#[derive(Clone, Debug, Serialize)]
pub(in crate::http_server) struct HostSummary {
    pub(in crate::http_server) name: String,
    pub(in crate::http_server) local: bool,
    pub(in crate::http_server) status: HostStatus,
    pub(in crate::http_server) machine_id: String,
    pub(in crate::http_server) version: String,
    pub(in crate::http_server) kernel: Option<String>,
    /// Capture time of the latest relayed snapshot; null while nobody watches.
    pub(in crate::http_server) captured_at: Option<u64>,
}

impl HostFeed {
    pub(super) fn new(info: HostInfo, link: u64) -> Arc<Self> {
        let (events, _) = broadcast::channel(16);
        Arc::new(Self {
            state: Mutex::new(FeedState {
                info,
                status: HostStatus::Online {
                    since: timestamp_ms(),
                },
                link,
                baseline: SnapshotBaseline::default(),
                sequence: 0,
                captured_at: None,
                viewers: 0,
            }),
            events,
            watched: watch::channel(false).0,
        })
    }

    pub(in crate::http_server) fn subscribe(
        self: &Arc<Self>,
    ) -> Result<FeedSubscription, FeedSubscribeError> {
        let mut state = self.state.lock().unwrap();
        if state.viewers >= VIEWER_LIMIT {
            return Err(FeedSubscribeError::TooManySubscribers);
        }
        state.viewers += 1;
        if state.viewers == 1 {
            self.watched.send_replace(true);
        }
        // Subscribing under the lock orders the initial state before every relayed event.
        Ok(FeedSubscription {
            feed: self.clone(),
            receiver: self.events.subscribe(),
            initial: state.baseline.full(state.sequence).to_string(),
            initial_sequence: state.sequence,
            status: state.status,
        })
    }

    /// The current merged state, for gap recovery of a lagging viewer.
    pub(in crate::http_server) fn full(&self) -> (u64, String) {
        let state = self.state.lock().unwrap();
        (
            state.sequence,
            state.baseline.full(state.sequence).to_string(),
        )
    }

    pub(in crate::http_server) fn summary(&self) -> HostSummary {
        let state = self.state.lock().unwrap();
        HostSummary {
            name: state.info.name.clone(),
            local: false,
            status: state.status,
            machine_id: state.info.machine_id.clone(),
            version: state.info.version.clone(),
            kernel: state.info.kernel.clone(),
            captured_at: state.captured_at,
        }
    }

    /// The merged snapshot of an online host, for cross-host correlation.
    pub(in crate::http_server) fn snapshot(&self) -> Option<serde_json::Map<String, Value>> {
        let state = self.state.lock().unwrap();
        state.captured_at.map(|_| state.baseline.snapshot().clone())
    }

    pub(super) fn info(&self) -> HostInfo {
        self.state.lock().unwrap().info.clone()
    }

    pub(super) fn online(&self) -> bool {
        matches!(self.state.lock().unwrap().status, HostStatus::Online { .. })
    }

    pub(super) fn idle_since(&self) -> Option<u64> {
        let state = self.state.lock().unwrap();
        match state.status {
            HostStatus::Offline { since } if state.viewers == 0 => Some(since),
            _ => None,
        }
    }

    pub(super) fn watched(&self) -> watch::Receiver<bool> {
        self.watched.subscribe()
    }

    pub(super) fn is_current(&self, link: u64) -> bool {
        self.state.lock().unwrap().link == link
    }

    /// Attaches a new agent link, replacing any previous one.
    pub(super) fn connect(&self, info: HostInfo, link: u64) {
        let mut state = self.state.lock().unwrap();
        state.info = info;
        state.link = link;
        state.baseline.clear();
        state.status = HostStatus::Online {
            since: timestamp_ms(),
        };
        let _ = self.events.send(FeedEvent::Status(state.status));
    }

    /// Marks the host offline unless a newer link already replaced `link`.
    pub(super) fn disconnect(&self, link: u64) {
        let mut state = self.state.lock().unwrap();
        if state.link != link {
            return;
        }
        state.status = HostStatus::Offline {
            since: timestamp_ms(),
        };
        let _ = self.events.send(FeedEvent::Status(state.status));
    }

    /// Merges and relays an agent snapshot payload from `link`. Returns true when
    /// the payload does not continue the merged state and a full snapshot is needed.
    pub(super) fn snapshot_frame(&self, link: u64, payload: &[u8]) -> Result<bool> {
        let Value::Object(payload) = serde_json::from_slice(payload)? else {
            anyhow::bail!("snapshot payload is not an object");
        };
        let mut state = self.state.lock().unwrap();
        if state.link != link || state.viewers == 0 {
            return Ok(false);
        }
        // A delta can follow a cleared baseline when viewers leave and return quickly.
        if !state.baseline.continues(&payload) {
            return Ok(true);
        }
        let sequence = state.sequence + 1;
        let relayed = state.baseline.apply(payload, sequence)?;
        state.sequence = sequence;
        state.captured_at = relayed["captured_at"].as_u64().or(state.captured_at);
        let _ = self.events.send(FeedEvent::Snapshot {
            sequence,
            data: relayed.to_string().into(),
        });
        Ok(false)
    }

    /// Relays an agent activity payload, re-encoded so it cannot break SSE framing.
    pub(super) fn activity_frame(&self, link: u64, payload: &[u8]) -> Result<()> {
        let activity: Value = serde_json::from_slice(payload)?;
        activity
            .as_object()
            .context("activity payload is not an object")?;
        if self.is_current(link) {
            let _ = self
                .events
                .send(FeedEvent::Activity(activity.to_string().into()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn info(name: &str) -> HostInfo {
        HostInfo {
            name: name.into(),
            machine_id: "m".into(),
            version: "v".into(),
            kernel: None,
        }
    }

    fn frame(value: Value) -> Vec<u8> {
        serde_json::to_vec(&value).unwrap()
    }

    #[test]
    fn viewers_drive_watch_and_receive_ordered_relayed_state() {
        let feed = HostFeed::new(info("b"), 1);
        let mut watched = feed.watched();
        assert!(!*watched.borrow_and_update());
        let full = frame(
            json!({"kind": "full", "sequence": 1, "captured_at": 5, "processes": [], "fd_relations": [{"id": "x"}]}),
        );
        // Frames for an unwatched host are dropped; the agent is not collecting.
        assert!(!feed.snapshot_frame(1, &full).unwrap());
        let mut first = feed.subscribe().unwrap();
        assert!(*watched.borrow_and_update());
        assert_eq!(first.initial_sequence, 0);
        feed.snapshot_frame(1, &full).unwrap();
        feed.snapshot_frame(2, &full).unwrap(); // a stale link
        feed.activity_frame(1, br#"{"cpu":[]}"#).unwrap();
        assert!(feed.activity_frame(1, b"[1]").is_err());
        assert!(feed.snapshot_frame(1, b"{").is_err());
        let FeedEvent::Snapshot { sequence, data } = first.receiver.try_recv().unwrap() else {
            panic!("snapshot expected");
        };
        assert_eq!(sequence, 1);
        assert_eq!(serde_json::from_str::<Value>(&data).unwrap()["sequence"], 1);
        assert!(matches!(
            first.receiver.try_recv().unwrap(),
            FeedEvent::Activity(_)
        ));
        assert!(first.receiver.try_recv().is_err());
        let second = feed.subscribe().unwrap();
        let initial: Value = serde_json::from_str(&second.initial).unwrap();
        assert_eq!(
            (initial["kind"].clone(), initial["sequence"].clone()),
            (json!("full"), json!(1))
        );
        assert_eq!(initial["fd_relations"], json!([{"id": "x"}]));
        assert_eq!(feed.summary().captured_at, Some(5));
        drop(first);
        drop(second);
        assert!(!*watched.borrow_and_update());
        assert_eq!(feed.summary().captured_at, None);
        assert_eq!(
            serde_json::from_str::<Value>(&feed.full().1).unwrap(),
            json!({"processes": [], "fd_relations": [], "rdma_relations": [], "rdma_ports": [], "warnings": [], "kind": "full", "sequence": 1})
        );
        // After the baseline is cleared, a delta asks for a full snapshot instead of failing.
        let _viewer = feed.subscribe().unwrap();
        let delta =
            frame(json!({"kind": "delta", "sequence": 2, "base_sequence": 1, "processes": []}));
        assert!(feed.snapshot_frame(1, &delta).unwrap());
        assert!(!feed.snapshot_frame(1, &full).unwrap());
    }

    #[test]
    fn links_replace_and_disconnect_only_the_current_one() {
        let feed = HostFeed::new(info("b"), 1);
        let mut viewer = feed.subscribe().unwrap();
        feed.connect(info("b"), 2);
        assert!(!feed.is_current(1) && feed.is_current(2));
        feed.disconnect(1);
        assert!(feed.online());
        feed.disconnect(2);
        assert!(!feed.online());
        let statuses: Vec<_> = std::iter::from_fn(|| viewer.receiver.try_recv().ok())
            .map(|event| match event {
                FeedEvent::Status(HostStatus::Online { .. }) => "online",
                FeedEvent::Status(HostStatus::Offline { .. }) => "offline",
                _ => "other",
            })
            .collect();
        assert_eq!(statuses, ["online", "offline"]);
        assert_eq!(feed.idle_since(), None, "a viewer keeps an offline host");
        drop(viewer);
        assert!(feed.idle_since().is_some());
        let viewers: Vec<_> = (0..VIEWER_LIMIT)
            .map(|_| feed.subscribe().unwrap())
            .collect();
        assert_eq!(
            feed.subscribe().err(),
            Some(FeedSubscribeError::TooManySubscribers)
        );
        drop(viewers);
    }
}
