use super::{AppState, hosts, router::router};
use anyhow::{Context, Result, ensure};
use std::{net::SocketAddr, sync::Arc, time::Duration};
use tokio::{sync::watch, task::JoinSet};

/// Validated startup options.
pub(crate) struct ServerConfig {
    pub(crate) listen: Vec<SocketAddr>,
    /// Agent link listeners; a non-empty list makes this server a hub.
    pub(crate) accept_agents: Vec<SocketAddr>,
    /// A hub address (`HOST:PORT`) this server pushes its observations to.
    pub(crate) connect: Option<String>,
    pub(crate) host_name: Option<String>,
    pub(crate) agent_token: Option<String>,
}

/// Bind every address before serving, then share collectors across listeners.
/// Each listener validates requests against its own bound address. A shutdown
/// signal or a listener failure stops all listeners, agent links and collectors.
pub(crate) async fn run(config: ServerConfig) -> Result<()> {
    ensure!(
        !config.listen.is_empty(),
        "no HTTP listen addresses specified"
    );
    let mut listeners = Vec::with_capacity(config.listen.len());
    for address in &config.listen {
        let listener = tokio::net::TcpListener::bind(address)
            .await
            .with_context(|| format!("could not bind HTTP listener on {address}"))?;
        let address = listener.local_addr()?;
        listeners.push((listener, address));
    }
    let mut agent_listeners = Vec::with_capacity(config.accept_agents.len());
    for address in &config.accept_agents {
        let listener = tokio::net::TcpListener::bind(address)
            .await
            .with_context(|| format!("could not bind agent listener on {address}"))?;
        agent_listeners.push(listener);
    }
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .context("could not install SIGTERM handler")?;
    let interval = Duration::from_secs(1);
    let local = hosts::HostInfo::local(config.host_name);
    let state = Arc::new(AppState::with_host(interval, local.clone()));
    let (shutdown, _) = watch::channel(false);
    let token: Option<Arc<str>> = config.agent_token.map(Into::into);
    let mut links = JoinSet::new();
    for listener in agent_listeners {
        log::info!(
            "procinsh {} accepting agents on {} as host {}",
            env!("CARGO_PKG_VERSION"),
            listener.local_addr()?,
            local.name
        );
        links.spawn(hosts::run_agent_listener(
            listener,
            state.hosts.clone(),
            token.clone(),
            shutdown.subscribe(),
        ));
    }
    if let Some(address) = config.connect {
        log::info!("agent {} linking to hub {address}", local.name);
        links.spawn(hosts::run_hub_connector(
            address,
            local,
            token,
            state.system_monitor.clone(),
            shutdown.subscribe(),
        ));
    }
    let mut servers = JoinSet::new();
    for (listener, address) in listeners {
        if !address.ip().is_loopback() {
            log::warn!(
                "Warning: remote access exposes process memory and environment variables without authentication or TLS. Use only on a trusted network."
            );
        }
        let app = router(state.clone(), address);
        let mut stop = shutdown.subscribe();
        servers.spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let _ = stop.changed().await;
                })
                .await
        });
        log::info!(
            "procinsh {} listening on http://{address} interval={:?}",
            env!("CARGO_PKG_VERSION"),
            interval
        );
    }
    let mut result = tokio::select! {
        result = tokio::signal::ctrl_c() => {
            match &result {
                Ok(()) => log::info!("received SIGINT; shutting down"),
                Err(error) => log::error!("SIGINT handler failed: {error}"),
            }
            result.context("SIGINT handler failed")
        },
        _ = terminate.recv() => {
            log::info!("received SIGTERM; shutting down");
            Ok(())
        },
        result = servers.join_next() => {
            match result {
                Some(Ok(result)) => result.context("HTTP server failed"),
                Some(Err(error)) => Err(error).context("HTTP server task failed"),
                None => Err(anyhow::anyhow!("no HTTP listeners")),
            }
        },
    };
    state.stop();
    shutdown.send_replace(true);
    // Agent link tasks return when they observe shutdown; sessions end with them.
    while links.join_next().await.is_some() {}
    while let Some(server) = servers.join_next().await {
        result = result.and(match server {
            Ok(result) => result.context("HTTP server failed"),
            Err(error) => Err(error).context("HTTP server task failed"),
        });
    }
    result = result.and(state.join_collectors());
    log::info!("procinsh stopped");
    result
}
