//! Process observation and collector lifecycle.
//!
//! # Interface
//!
//! | Definition | Visibility | Kind / signature |
//! | --- | --- | --- |
//! | [`ObservationSession`] | `pub(in crate::helper)` | `struct ObservationSession` |
//! | [`ProcessMonitor`] | `pub(in crate::helper)` | `struct `[`ProcessMonitor`] |
//! | [`capture_sample`] | `pub(super)` | `fn capture_sample(id: ProcessId) -> anyhow::Result<ProcessSample>` |
//!
//! Sampling returns raw [`sampling::ProcessSample`] counters with monotonic times.
//! [`ProcessMonitor`] owns acquisition sessions; main derives observations and histories.

#[cfg(test)]
mod history;
mod sampling;
mod service;
pub(super) use sampling::capture_sample;
pub(in crate::helper) use service::{ObservationSession, ProcessMonitor};
