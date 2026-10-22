//! A remote host's latest snapshot, rebuilt from the agent's full and delta payloads.
//!
//! The rules mirror the browser's `mergeSnapshot`: maps merge by start address,
//! FD and RDMA relations by ID, and omitted maps keep the same process's maps.
//! Browsers receive the hub's own per-host sequence numbers, so every viewer can
//! join from the merged state and then follow the relayed deltas.

use anyhow::{Context, Result, bail, ensure};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub(super) struct SnapshotBaseline {
    /// The merged snapshot in full form, without kind and sequence fields.
    full: Map<String, Value>,
    /// The agent's sequence of the merged state.
    agent_sequence: Option<u64>,
}

fn identity(process: &Value) -> Option<(u64, u64)> {
    let id = process.get("identity")?;
    Some((
        id.get("pid")?.as_u64()?,
        id.get("start_time_ticks")?.as_u64()?,
    ))
}

/// Applies an ID-keyed delta, keeping existing entries in place and appending new ones.
fn apply_entries(old: &[Value], delta: &Value, key: &str) -> Result<Vec<Value>> {
    let removed: HashSet<&str> = delta["remove"]
        .as_array()
        .context("delta without remove")?
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let mut entries: Vec<Value> = old
        .iter()
        .filter(|entry| !entry[key].as_str().is_some_and(|id| removed.contains(id)))
        .cloned()
        .collect();
    let mut index: HashMap<String, usize> = entries
        .iter()
        .enumerate()
        .filter_map(|(i, entry)| Some((entry[key].as_str()?.to_owned(), i)))
        .collect();
    for entry in delta["upsert"].as_array().context("delta without upsert")? {
        let id = entry[key].as_str().context("delta entry without key")?;
        match index.get(id) {
            Some(&i) => entries[i] = entry.clone(),
            None => {
                index.insert(id.to_owned(), entries.len());
                entries.push(entry.clone());
            }
        }
    }
    Ok(entries)
}

fn collection(
    payload: &Map<String, Value>,
    previous: &Map<String, Value>,
    name: &str,
    key: &str,
) -> Result<Value> {
    if let Some(full) = payload.get(name) {
        ensure!(full.is_array(), "{name} is not an array");
        return Ok(full.clone());
    }
    let old = previous
        .get(name)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    Ok(Value::Array(match payload.get(&format!("{name}_delta")) {
        Some(delta) => apply_entries(old, delta, key)?,
        None => old.to_vec(),
    }))
}

impl SnapshotBaseline {
    pub(super) fn clear(&mut self) {
        *self = Self::default();
    }

    /// Whether `payload` is a full snapshot or a delta from the merged state.
    pub(super) fn continues(&self, payload: &Map<String, Value>) -> bool {
        match payload.get("kind").and_then(Value::as_str) {
            Some("delta") => {
                self.agent_sequence.is_some()
                    && payload.get("base_sequence").and_then(Value::as_u64) == self.agent_sequence
            }
            _ => true,
        }
    }

    /// Merges one agent payload and returns it renumbered for browsers:
    /// `sequence` and, for a delta, `base_sequence = sequence - 1`.
    pub(super) fn apply(
        &mut self,
        mut payload: Map<String, Value>,
        sequence: u64,
    ) -> Result<Value> {
        let kind = payload
            .get("kind")
            .and_then(Value::as_str)
            .context("snapshot without kind")?;
        let agent_sequence = payload
            .get("sequence")
            .and_then(Value::as_u64)
            .context("snapshot without sequence")?;
        let full = match kind {
            "full" => true,
            "delta" => {
                ensure!(
                    self.agent_sequence.is_some()
                        && payload.get("base_sequence").and_then(Value::as_u64)
                            == self.agent_sequence,
                    "agent snapshot baseline mismatch"
                );
                false
            }
            _ => bail!("unknown snapshot kind {kind}"),
        };
        let previous = if full {
            Map::new()
        } else {
            std::mem::take(&mut self.full)
        };
        let old_processes: HashMap<_, _> = previous
            .get("processes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|p| Some((identity(p)?, p)))
            .collect();
        let mut merged = payload.clone();
        for name in ["kind", "sequence", "base_sequence"] {
            merged.remove(name);
        }
        let processes = payload
            .get("processes")
            .and_then(Value::as_array)
            .context("snapshot without processes")?;
        let mut rebuilt = Vec::with_capacity(processes.len());
        for process in processes {
            let mut process = process
                .as_object()
                .context("process is not an object")?
                .clone();
            let old =
                identity(&Value::Object(process.clone())).and_then(|id| old_processes.get(&id));
            let maps = match (process.remove("maps"), process.remove("maps_delta")) {
                (Some(maps), _) => maps,
                (None, Some(delta)) => {
                    let old = old.context("maps delta without baseline")?;
                    let mut maps = apply_entries(
                        old["maps"].as_array().context("baseline maps")?,
                        &delta,
                        "start",
                    )?;
                    maps.sort_by_key(|m| {
                        m["start"]
                            .as_str()
                            .and_then(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).ok())
                    });
                    Value::Array(maps)
                }
                (None, None) => {
                    process.insert(
                        "maps_epoch".into(),
                        old.map_or(0.into(), |old| old["maps_epoch"].clone()),
                    );
                    old.map_or(Value::Array(vec![]), |old| old["maps"].clone())
                }
            };
            process.insert("maps".into(), maps);
            rebuilt.push(Value::Object(process));
        }
        merged.insert("processes".into(), Value::Array(rebuilt));
        for (name, key) in [("fd_relations", "id"), ("rdma_relations", "id")] {
            merged.remove(&format!("{name}_delta"));
            merged.insert(name.into(), collection(&payload, &previous, name, key)?);
        }
        if !merged.contains_key("rdma_ports") {
            merged.insert(
                "rdma_ports".into(),
                previous
                    .get("rdma_ports")
                    .cloned()
                    .unwrap_or(Value::Array(vec![])),
            );
        }
        self.full = merged;
        self.agent_sequence = Some(agent_sequence);
        payload.insert("sequence".into(), sequence.into());
        if full {
            payload.remove("base_sequence");
        } else {
            payload.insert("base_sequence".into(), (sequence - 1).into());
        }
        Ok(Value::Object(payload))
    }

    /// The merged state as a full snapshot with the hub's sequence.
    pub(super) fn full(&self, sequence: u64) -> Value {
        let mut full = self.full.clone();
        for (name, empty) in [
            ("processes", Value::Array(vec![])),
            ("fd_relations", Value::Array(vec![])),
            ("rdma_relations", Value::Array(vec![])),
            ("rdma_ports", Value::Array(vec![])),
            ("warnings", Value::Array(vec![])),
        ] {
            full.entry(name).or_insert(empty);
        }
        full.insert("kind".into(), "full".into());
        full.insert("sequence".into(), sequence.into());
        Value::Object(full)
    }

    /// The merged snapshot object, for cross-host correlation.
    pub(super) fn snapshot(&self) -> &Map<String, Value> {
        &self.full
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn object(value: Value) -> Map<String, Value> {
        value.as_object().unwrap().clone()
    }

    #[test]
    fn deltas_merge_like_the_browser_and_are_renumbered() {
        let map = |start: &str, writable: bool| json!({"start": start, "writable": writable});
        let process = |pid: u64, maps: Option<Value>| {
            let mut p = json!({"identity": {"pid": pid, "start_time_ticks": 1}, "name": "p", "maps_epoch": pid});
            if let Some(maps) = maps {
                p["maps"] = maps;
            }
            p
        };
        let mut baseline = SnapshotBaseline::default();
        let full = baseline
            .apply(
                object(json!({
                    "kind": "full", "sequence": 7, "captured_at": 1,
                    "processes": [process(1, Some(json!([map("0x1000", false), map("0x3000", false)]))), process(2, Some(json!([])))],
                    "fd_relations": [{"id": "a"}, {"id": "b"}],
                    "rdma_relations": [{"id": "q"}],
                    "rdma_ports": [{"device": "ib0"}],
                    "warnings": [],
                })),
                100,
            )
            .unwrap();
        assert_eq!(
            (full["sequence"].clone(), full.get("base_sequence")),
            (json!(100), None)
        );
        let delta = baseline
            .apply(
                object(json!({
                    "kind": "delta", "sequence": 8, "base_sequence": 7, "captured_at": 2,
                    "processes": [
                        {"identity": {"pid": 1, "start_time_ticks": 1}, "name": "p", "maps_epoch": 9,
                         "maps_delta": {"upsert": [map("0x2000", true), map("0x1000", true)], "remove": ["0x3000"]}},
                        {"identity": {"pid": 2, "start_time_ticks": 1}, "name": "p", "maps_epoch": 99},
                    ],
                    "fd_relations_delta": {"upsert": [{"id": "c"}, {"id": "a", "x": 1}], "remove": ["b"]},
                    "rdma_relations_delta": {"upsert": [], "remove": ["q"]},
                    "warnings": ["w"],
                })),
                101,
            )
            .unwrap();
        assert_eq!(delta["sequence"], 101);
        assert_eq!(delta["base_sequence"], 100);
        assert!(
            delta.get("fd_relations_delta").is_some(),
            "relayed as a delta"
        );
        let merged = baseline.full(101);
        assert_eq!(merged["kind"], "full");
        assert_eq!(
            merged["processes"][0]["maps"],
            json!([map("0x1000", true), map("0x2000", true)])
        );
        // Omitted maps keep both the maps and their epoch.
        assert_eq!(merged["processes"][1]["maps"], json!([]));
        assert_eq!(merged["processes"][1]["maps_epoch"], 2);
        assert_eq!(
            merged["fd_relations"],
            json!([{"id": "a", "x": 1}, {"id": "c"}])
        );
        assert_eq!(merged["rdma_relations"], json!([]));
        assert_eq!(merged["rdma_ports"], json!([{"device": "ib0"}]));
        assert_eq!(merged["warnings"], json!(["w"]));
        assert!(merged.get("fd_relations_delta").is_none());
        // A delta that skips the agent's previous sequence is rejected.
        let skipped =
            object(json!({"kind": "delta", "sequence": 10, "base_sequence": 9, "processes": []}));
        assert!(baseline.apply(skipped, 102).is_err());
        let mut empty = SnapshotBaseline::default();
        assert!(
            empty
                .apply(
                    object(
                        json!({"kind": "delta", "sequence": 1, "base_sequence": 0, "processes": []})
                    ),
                    1
                )
                .is_err()
        );
        assert_eq!(empty.full(0)["processes"], json!([]));
        assert!(
            empty
                .apply(
                    object(json!({"kind": "other", "sequence": 1, "processes": []})),
                    1
                )
                .is_err()
        );
    }
}
