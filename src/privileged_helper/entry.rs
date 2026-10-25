//! Private privileged acquisition executable. It has no HTTP listener.
//!
//! # Interface
//! [`ipc`] (`pub`): `mod ipc`, shared native values and transport.
#[cfg(test)]
#[path = "../main/analysis/root.rs"]
pub mod analysis;
#[path = "../ipc/root.rs"]
pub mod ipc;
#[cfg(test)]
#[path = "../main/json.rs"]
mod json;
#[cfg(test)]
#[path = "../main/model/root.rs"]
pub mod model;
#[path = "root.rs"]
mod privileged_helper;

fn main() -> std::process::ExitCode {
    env_logger::init();
    match privileged_helper::run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("privileged helper: {error:#}");
            std::process::ExitCode::FAILURE
        }
    }
}
