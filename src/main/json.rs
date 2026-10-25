//! Main-only JSON and query representations for IPC identities and acquisition values.
//!
//! # Interface
//! Installs serde implementations; no named types or functions are exported.
#[path = "json/acquisition.rs"]
mod acquisition;

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(remote = "crate::ipc::schema::ProcessId", deny_unknown_fields)]
struct ProcessIdJson {
    pid: i32,
    start_time_ticks: u64,
}
impl serde::Serialize for crate::ipc::schema::ProcessId {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        ProcessIdJson::serialize(self, s)
    }
}
impl<'de> serde::Deserialize<'de> for crate::ipc::schema::ProcessId {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        ProcessIdJson::deserialize(d)
    }
}
