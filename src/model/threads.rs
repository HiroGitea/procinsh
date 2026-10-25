//! Raw thread counters and interpreted thread values.
//!
//! # Interface
//!
//! | Definition | Visibility | Kind / signature |
//! | --- | --- | --- |
//! | [`SchedulerPolicy`] | `pub` | `enum SchedulerPolicy` |
//! | [`CpuRange`] | `pub` | `struct CpuRange` |
//! | [`affinity`] | `pub` | `fn affinity(text: &str) -> Option<Vec<CpuRange>>` |
//! | [`ThreadObservation`] | `pub` | `struct ThreadObservation` |
//! | [`ThreadSample`] | `pub` | `struct ThreadSample` |

use serde::Serialize;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(tag = "kind", content = "code", rename_all = "snake_case")]
pub enum SchedulerPolicy {
    Other,
    Fifo,
    Rr,
    Batch,
    Idle,
    Deadline,
    Ext,
    Unknown(u32),
}
impl SchedulerPolicy {
    pub fn from_code(code: u32) -> Self {
        match code {
            0 => Self::Other,
            1 => Self::Fifo,
            2 => Self::Rr,
            3 => Self::Batch,
            5 => Self::Idle,
            6 => Self::Deadline,
            7 => Self::Ext,
            n => Self::Unknown(n),
        }
    }
}
/// Inclusive range of CPU indices in a thread's affinity mask.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct CpuRange {
    pub start: u32,
    pub end: u32,
}

pub fn affinity(text: &str) -> Option<Vec<CpuRange>> {
    text.split(',')
        .map(|item| {
            let (start, end) = item.split_once('-').unwrap_or((item, item));
            let start = start.parse().ok()?;
            let end = end.parse().ok()?;
            (start <= end).then_some(CpuRange { start, end })
        })
        .collect()
}
#[derive(Clone, Debug, Serialize, serde::Deserialize)]
pub struct ThreadObservation {
    pub tid: i32,
    pub name: String,
    pub state: String,
    pub cpu: i32,
    pub cpu_percent: Option<f64>,
    pub priority: i64,
    pub nice: i64,
    pub scheduler: SchedulerPolicy,
    pub affinity: Option<Vec<CpuRange>>,
    pub voluntary_context_switches: Option<u64>,
    pub nonvoluntary_context_switches: Option<u64>,
}

/// Raw thread metadata and counters; CPU percentages are derived between observations.
#[derive(Clone, Debug, Serialize, serde::Deserialize)]
pub struct ThreadSample {
    pub tid: i32,
    pub name: String,
    pub state: String,
    pub cpu: i32,
    pub priority: i64,
    pub nice: i64,
    pub scheduler: SchedulerPolicy,
    pub affinity: Option<Vec<CpuRange>>,
    pub voluntary_context_switches: Option<u64>,
    pub nonvoluntary_context_switches: Option<u64>,
    pub ticks: u64,
    pub start_time: u64,
}

impl ThreadSample {
    pub fn observation(&self) -> ThreadObservation {
        ThreadObservation {
            tid: self.tid,
            name: self.name.clone(),
            state: self.state.clone(),
            cpu: self.cpu,
            priority: self.priority,
            nice: self.nice,
            scheduler: self.scheduler,
            affinity: self.affinity.clone(),
            voluntary_context_switches: self.voluntary_context_switches,
            nonvoluntary_context_switches: self.nonvoluntary_context_switches,
            cpu_percent: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn affinity_ranges_and_unknown_scheduler_keep_values() {
        assert_eq!(
            affinity("0-3,8"),
            Some(vec![
                CpuRange { start: 0, end: 3 },
                CpuRange { start: 8, end: 8 }
            ])
        );
        assert_eq!(
            affinity("4294967295"),
            Some(vec![CpuRange {
                start: u32::MAX,
                end: u32::MAX
            }])
        );
        for text in ["", "3-1", "1,", "1-2-3", "4294967296", "x"] {
            assert!(affinity(text).is_none());
        }
        assert_eq!(
            serde_json::to_value(SchedulerPolicy::from_code(99)).unwrap(),
            serde_json::json!({"kind":"unknown","code":99})
        );
    }
}
