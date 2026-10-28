//! Process inspector for Linux.
//!
//! Start with the [HTTP application interface](http_server). Each subsystem's
//! **Interface** section lists its re-exported types and function signatures,
//! with links to the defining items and their source.
//!
//! # Interface
//! No items are exported. Startup delegates to [`http_server::run`].

#[path = "root.rs"]
mod http_server;
#[path = "../ipc/root.rs"]
mod ipc;
#[cfg(test)]
#[path = "../test_support.rs"]
mod test_support;

#[cfg(not(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
)))]
compile_error!("procinsh supports Linux x86-64 and AArch64 only");

use anyhow::{Context, Result, ensure};
use clap::Parser;
use std::{io::Write, net::SocketAddr, path::PathBuf};

#[derive(Parser)]
#[cfg_attr(
    target_arch = "x86_64",
    command(version, about = "Read-only Linux x86-64 process inspector")
)]
#[cfg_attr(
    target_arch = "aarch64",
    command(version, about = "Read-only Linux AArch64 process inspector")
)]
struct Cli {
    /// Listen address (repeat for multiple listeners); non-loopback addresses require --allow-non-loopback
    #[arg(long, default_value = "127.0.0.1:8080")]
    listen: Vec<SocketAddr>,
    /// Allow non-loopback listening: exposes process memory and environment variables without authentication or TLS
    #[arg(long)]
    allow_non_loopback: bool,
    /// Accept agent links on this address and show their hosts (repeatable); non-loopback addresses also require an agent token
    #[arg(long, value_name = "ADDRESS")]
    accept_agents: Vec<SocketAddr>,
    /// Link to a hub's agent address and answer its requests for this host
    #[arg(long, value_name = "HOST:PORT")]
    connect: Option<String>,
    /// Host name shown by a hub (default: the kernel host name); letters, digits, '.', '_' and '-'
    #[arg(long, value_name = "NAME")]
    host_name: Option<String>,
    /// File holding the shared agent token
    #[arg(long, value_name = "PATH")]
    agent_token_file: Option<PathBuf>,
}

/// The agent token comes from a file, never from arguments or the environment,
/// because procinsh itself shows every process's command line and environment.
fn agent_token(cli: &Cli) -> Result<Option<String>> {
    let Some(path) = &cli.agent_token_file else {
        return Ok(None);
    };
    let token = std::fs::read_to_string(path)
        .with_context(|| format!("could not read agent token file {}", path.display()))?;
    let token = token.trim_end_matches(['\r', '\n']).to_owned();
    ensure!(!token.is_empty(), "the agent token is empty");
    Ok(Some(token))
}

fn main() -> std::process::ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .target(env_logger::Target::Stderr)
        .format(|buffer, record| {
            writeln!(
                buffer,
                "[{} {:5} {}:{}] {}",
                buffer.timestamp_millis(),
                record.level(),
                record.file().unwrap_or("unknown"),
                record.line().unwrap_or(0),
                record.args()
            )
        })
        .init();
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            log::error!("{error:#}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    for address in &cli.listen {
        ensure!(
            address.ip().is_loopback() || cli.allow_non_loopback,
            "refusing to listen on non-loopback address {}; use --allow-non-loopback to explicitly expose process memory and environment variables without authentication or TLS",
            address
        );
    }
    let agent_token = agent_token(&cli)?;
    for address in &cli.accept_agents {
        ensure!(
            address.ip().is_loopback() || (cli.allow_non_loopback && agent_token.is_some()),
            "refusing to accept agents on non-loopback address {}; use --allow-non-loopback and an agent token (--agent-token-file or PROCINSH_AGENT_TOKEN)",
            address
        );
    }
    if let Some(name) = &cli.host_name {
        ensure!(
            http_server::valid_host_name(name),
            "invalid host name {name:?}: use 1-63 letters, digits, '.', '_' or '-', not \"all\""
        );
    }
    http_server::run(http_server::ServerConfig {
        listen: cli.listen,
        accept_agents: cli.accept_agents,
        connect: cli.connect,
        host_name: cli.host_name,
        agent_token,
    })
}
