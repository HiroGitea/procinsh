//! The hub side of agent links.

use super::feed::HostFeed;
use super::identity::HostInfo;
use super::registry::HostRegistry;
use super::wire::{self, Control, FrameKind};
use anyhow::{Context, Result, bail};
use std::{net::SocketAddr, sync::Arc, time::Duration};
use tokio::{
    net::{TcpListener, TcpStream, tcp::OwnedWriteHalf},
    sync::{mpsc, watch},
    time::{Instant, timeout},
};

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
const PING_INTERVAL: Duration = Duration::from_secs(10);
/// A link without any frame for this long is closed.
const IDLE_TIMEOUT: Duration = Duration::from_secs(30);

/// Accepts agent links until `shutdown` changes. `token`, when set, must match each
/// agent's hello.
pub(in crate::http_server) async fn run_agent_listener(
    listener: TcpListener,
    registry: Arc<HostRegistry>,
    token: Option<Arc<str>>,
    mut shutdown: watch::Receiver<bool>,
) {
    loop {
        let (stream, peer) = tokio::select! {
            accepted = listener.accept() => match accepted {
                Ok(accepted) => accepted,
                Err(error) => {
                    log::warn!("agent accept failed: {error}");
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                }
            },
            _ = shutdown.changed() => return,
        };
        let (registry, token, shutdown) = (registry.clone(), token.clone(), shutdown.clone());
        tokio::spawn(async move {
            if let Err(error) = serve_agent(stream, peer, &registry, token, shutdown).await {
                log::warn!("agent link from {peer} closed: {error:#}");
            }
        });
    }
}

async fn handshake(
    stream: &mut TcpStream,
    registry: &HostRegistry,
    token: Option<&str>,
) -> Result<(Arc<HostFeed>, u64)> {
    let (kind, payload) = timeout(HANDSHAKE_TIMEOUT, wire::read_frame(stream))
        .await
        .context("agent hello timed out")??;
    let hello = (kind == FrameKind::Control)
        .then(|| wire::parse_control(&payload))
        .transpose()?;
    let Some(Control::Hello {
        protocol,
        host,
        machine_id,
        version,
        kernel,
        token: offered,
    }) = hello
    else {
        bail!("agent did not start with hello");
    };
    let rejection = if protocol != wire::PROTOCOL {
        Some(format!(
            "agent protocol {protocol} is not supported (hub protocol {})",
            wire::PROTOCOL
        ))
    } else if let Some(expected) = token
        && !wire::same_secret(expected, offered.as_deref().unwrap_or_default())
    {
        Some("agent token mismatch".into())
    } else {
        None
    };
    let admitted = match rejection {
        Some(reason) => Err(reason),
        None => registry.admit(HostInfo {
            name: host,
            machine_id,
            version,
            kernel,
        }),
    };
    match admitted {
        Ok(admitted) => {
            wire::write_control(
                stream,
                &Control::Welcome {
                    protocol: wire::PROTOCOL,
                    version: env!("CARGO_PKG_VERSION").into(),
                },
            )
            .await?;
            Ok(admitted)
        }
        Err(reason) => {
            wire::write_control(
                stream,
                &Control::Reject {
                    reason: reason.clone(),
                },
            )
            .await?;
            bail!("rejected agent: {reason}");
        }
    }
}

async fn serve_agent(
    mut stream: TcpStream,
    peer: SocketAddr,
    registry: &HostRegistry,
    token: Option<Arc<str>>,
    mut shutdown: watch::Receiver<bool>,
) -> Result<()> {
    stream.set_nodelay(true)?;
    let (feed, link) = handshake(&mut stream, registry, token.as_deref()).await?;
    let name = feed.info().name;
    log::info!("agent {name} connected from {peer}");
    let result = relay(stream, &feed, link, &mut shutdown).await;
    registry.disconnected(&feed, link);
    log::info!("agent {name} disconnected");
    result
}

async fn send_watch(
    writer: &mut OwnedWriteHalf,
    watched: &mut watch::Receiver<bool>,
) -> Result<()> {
    let active = *watched.borrow_and_update();
    wire::write_control(writer, &Control::Watch { active }).await
}

/// Relays agent frames into `feed` until the link fails, idles, is replaced, or
/// the hub shuts down.
async fn relay(
    stream: TcpStream,
    feed: &HostFeed,
    link: u64,
    shutdown: &mut watch::Receiver<bool>,
) -> Result<()> {
    let (mut reader, mut writer) = stream.into_split();
    // A dedicated reader keeps partially read frames intact across select branches.
    let (frames, mut incoming) = mpsc::channel(8);
    let reading = tokio::spawn(async move {
        loop {
            let frame = wire::read_frame(&mut reader).await;
            let failed = frame.is_err();
            if frames.send(frame).await.is_err() || failed {
                break;
            }
        }
    });
    let mut watched = feed.watched();
    send_watch(&mut writer, &mut watched).await?;
    let mut ping = tokio::time::interval(PING_INTERVAL);
    let mut last_frame = Instant::now();
    let result = loop {
        tokio::select! {
            frame = incoming.recv() => {
                let Some(frame) = frame else { break Ok(()) };
                let (kind, payload) = match frame {
                    Ok(frame) => frame,
                    Err(error) => break Err(error),
                };
                last_frame = Instant::now();
                let handled = match kind {
                    FrameKind::Snapshot => match feed.snapshot_frame(link, &payload) {
                        Ok(true) => wire::write_control(&mut writer, &Control::Resync).await,
                        other => other.map(|_| ()),
                    },
                    FrameKind::Activity => feed.activity_frame(link, &payload),
                    FrameKind::Control => match wire::parse_control(&payload) {
                        Ok(Control::Ping) => wire::write_control(&mut writer, &Control::Pong).await,
                        Ok(_) => Ok(()),
                        Err(error) => Err(error),
                    },
                };
                if let Err(error) = handled {
                    break Err(error);
                }
            }
            changed = watched.changed() => {
                if changed.is_err() { break Ok(()) }
                if let Err(error) = send_watch(&mut writer, &mut watched).await {
                    break Err(error);
                }
            }
            _ = ping.tick() => {
                if !feed.is_current(link) {
                    break Ok(());
                }
                if last_frame.elapsed() > IDLE_TIMEOUT {
                    break Err(anyhow::anyhow!("agent link idle for {IDLE_TIMEOUT:?}"));
                }
                if let Err(error) = wire::write_control(&mut writer, &Control::Ping).await {
                    break Err(error);
                }
            }
            _ = shutdown.changed() => break Ok(()),
        }
    };
    reading.abort();
    result
}
