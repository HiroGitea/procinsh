//! Closed monitor operations and structured replies. No request accepts a path,
//! arbitrary memory read, syscall number, file descriptor, or caller-selected limit.
//! Mapped ELF requests select an existing executable mapping by its start address;
//! the helper independently verifies the process and mapped file identity.
//!
//! Frames have a four-byte native-endian length and a versioned binary payload.
//! Requests are at most 1 KiB,
//! responses at most 1 GiB, and at most 64 requests may queue in the application.
//! Socket I/O and application requests have 30-second deadlines. Each side
//! treats an invalid/truncated frame or a timeout as a fatal connection error.
//! Sessions expire after 15 seconds without polling and are capped at 32 per
//! kind. Event polling never blocks waiting for a new observation.
//!
//! # Interface
//!
//! | Definition | Visibility | Kind / type |
//! | --- | --- | --- |
//! | [`ProcessId`] | `pub` | `struct ProcessId` |
//! | [`HelperIdentity`] | `pub` | `struct HelperIdentity` |
//! | [`ProcessResource`] | `pub` | `enum ProcessResource` |
//! | [`MonitorRequest`] | `pub` | `enum MonitorRequest` |
//! | [`MonitorEvent`] | `pub` | `enum MonitorEvent` |
//! | [`MonitorResponse`] | `pub` | `enum MonitorResponse` |
//! | [`WireResponse`] | `pub` | `type WireResponse = Result<`[`MonitorResponse`]`, String>` |
//!
//! Target identities use [`ProcessId`]. Acquisition snapshots use
//! [`SystemSnapshot`](crate::model::system::SystemSnapshot); HTTP-shaped values use [`Value`].

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

pub const MAX_REQUEST: usize = 1024;
pub const MAX_RESPONSE: usize = 1024 * 1024 * 1024;
pub const TIMEOUT: Duration = Duration::from_secs(30);
pub const SESSION_LEASE: Duration = Duration::from_secs(15);

#[derive(Clone, Copy)]
pub enum ProcessResource {
    Observation,
    Threads,
    Maps,
    Environment,
    Auxv,
    Fds,
}

pub enum MonitorRequest {
    Processes,
    Process {
        id: ProcessId,
        resource: ProcessResource,
    },
    MappedElf {
        id: ProcessId,
        mapping_start: u64,
        known_file: Option<crate::analysis::snapshot::symbol::ElfFileId>,
    },
    ObserveProcess {
        id: ProcessId,
    },
    ObserveSystem,
    Poll {
        session: u64,
    },
    Close {
        session: u64,
    },
}

pub enum MonitorEvent {
    Observation(Value),
    Snapshot(Value),
    AcquiredSnapshot(std::sync::Arc<crate::model::system::SystemSnapshot>),
    AcquisitionGap {
        dropped: u64,
        snapshot: std::sync::Arc<crate::model::system::SystemSnapshot>,
    },
    Activity(Value),
    Gap {
        dropped: u64,
        snapshot: Value,
    },
}

pub enum MonitorResponse {
    Data(Value),
    /// Raw ELF bytes in the native binary payload.
    Elf(Option<crate::analysis::snapshot::symbol::ElfFile>),
    ElfUnchanged,
    Started {
        session: u64,
        initial: MonitorEvent,
    },
    Event {
        event: Option<MonitorEvent>,
        closed: bool,
    },
    Closed,
}

/// Errors are data on the wire; neither HTTP status codes nor HTTP syntax are
/// interpreted in the helper. The HTTP layer chooses the response status.
pub type WireResponse = Result<MonitorResponse, String>;

/// PID plus kernel start time, independently validated by the helper.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(deny_unknown_fields)]
pub struct ProcessId {
    pub pid: i32,
    pub start_time_ticks: u64,
}

/// Startup handshake tied to the child returned by Command::spawn.
pub struct HelperIdentity {
    pub pid: u32,
    pub version: String,
    pub capabilities: u64,
}

crate::ipc::codec::wire_struct!(ProcessId {
    pid,
    start_time_ticks
});
crate::ipc::codec::wire_struct!(HelperIdentity {
    pid,
    version,
    capabilities
});
crate::ipc::codec::wire_enum!(ProcessResource { 0 => Observation, 1 => Threads, 2 => Maps, 3 => Environment, 4 => Auxv, 5 => Fds });
crate::ipc::codec::wire_enum!(MonitorRequest { 0 => Processes, 1 => Process{id,resource}, 2 => MappedElf{id,mapping_start,known_file}, 3 => ObserveProcess{id}, 4 => ObserveSystem, 5 => Poll{session}, 6 => Close{session} });
crate::ipc::codec::wire_enum!(MonitorEvent { 0 => Observation(value0), 1 => Snapshot(value0), 2 => AcquiredSnapshot(value0), 3 => AcquisitionGap{dropped,snapshot}, 4 => Activity(value0), 5 => Gap{dropped,snapshot} });
crate::ipc::codec::wire_enum!(MonitorResponse { 0 => Data(value0), 1 => Elf(value0), 2 => ElfUnchanged, 3 => Started{session,initial}, 4 => Event{event,closed}, 5 => Closed });
