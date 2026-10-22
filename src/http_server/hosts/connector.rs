//! The agent side of a hub link: reconnects with backoff and pushes this host's
//! snapshots and activity while the hub has viewers for it.

use super::identity::HostInfo;
use super::wire::{self, Control, FrameKind};
use crate::http_server::system::{
    SnapshotEncoder, SubscribeError, Subscription, SystemMonitor, SystemMonitorEvent,
};
use anyhow::{Context, Result, bail};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    net::{TcpStream, tcp::OwnedWriteHalf},
    sync::{broadcast::error::RecvError, mpsc, watch},
    time::timeout,
};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const PING_INTERVAL: Duration = Duration::from_secs(10);
const IDLE_TIMEOUT: Duration = Duration::from_secs(30);
const MIN_BACKOFF: Duration = Duration::from_secs(1);
const MAX_BACKOFF: Duration = Duration::from_secs(30);

/// A session that lasted this long resets the reconnect backoff.
const STABLE_SESSION: Duration = Duration::from_secs(30);

/// Keeps a link to the hub at `address` until `shutdown` changes.
pub(in crate::http_server) async fn run_hub_connector(
    address: String,
    info: HostInfo,
    token: Option<Arc<str>>,
    monitor: Arc<SystemMonitor>,
    mut shutdown: watch::Receiver<bool>,
) {
    let mut backoff = MIN_BACKOFF;
    loop {
        let started = Instant::now();
        let result = tokio::select! {
            result = session(&address, &info, token.as_deref(), &monitor) => result,
            _ = shutdown.changed() => return,
        };
        match result {
            Ok(()) => log::info!("hub {address} closed the agent link"),
            Err(error) => log::warn!("agent link to hub {address}: {error:#}"),
        }
        if started.elapsed() >= STABLE_SESSION {
            backoff = MIN_BACKOFF;
        }
        // Up to 20% jitter keeps many agents from reconnecting in step.
        let jitter = backoff.mul_f64(f64::from(std::process::id() % 21) / 100.0);
        tokio::select! {
            _ = tokio::time::sleep(backoff + jitter) => {}
            _ = shutdown.changed() => return,
        }
        backoff = (backoff * 2).min(MAX_BACKOFF);
    }
}

async fn send_snapshot(
    writer: &mut OwnedWriteHalf,
    encoder: &mut SnapshotEncoder,
    snapshot: Arc<crate::http_server::system::SystemSnapshot>,
    full: bool,
) -> Result<()> {
    let data = encoder.encode(snapshot, full, std::time::Instant::now())?;
    wire::write_frame(writer, FrameKind::Snapshot, data.as_bytes()).await
}

async fn session(
    address: &str,
    info: &HostInfo,
    token: Option<&str>,
    monitor: &Arc<SystemMonitor>,
) -> Result<()> {
    let mut stream = timeout(CONNECT_TIMEOUT, TcpStream::connect(address))
        .await
        .context("connect timed out")??;
    stream.set_nodelay(true)?;
    wire::write_control(
        &mut stream,
        &Control::Hello {
            protocol: wire::PROTOCOL,
            host: info.name.clone(),
            machine_id: info.machine_id.clone(),
            version: info.version.clone(),
            kernel: info.kernel.clone(),
            token: token.map(str::to_owned),
        },
    )
    .await?;
    let (kind, payload) = timeout(CONNECT_TIMEOUT, wire::read_frame(&mut stream))
        .await
        .context("hub welcome timed out")??;
    match (kind == FrameKind::Control)
        .then(|| wire::parse_control(&payload))
        .transpose()?
    {
        Some(Control::Welcome { .. }) => {}
        Some(Control::Reject { reason }) => bail!("hub rejected this agent: {reason}"),
        _ => bail!("hub did not answer hello"),
    }
    log::info!("agent {} linked to hub {address}", info.name);
    let (mut reader, mut writer) = stream.into_split();
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
    let mut subscription: Option<Subscription> = None;
    let mut encoder = SnapshotEncoder::default();
    let mut ping = tokio::time::interval(PING_INTERVAL);
    let mut last_frame = Instant::now();
    let result: Result<()> = async {
        loop {
            tokio::select! {
                frame = incoming.recv() => {
                    let Some(frame) = frame else { return Ok(()) };
                    let (kind, payload) = frame?;
                    last_frame = Instant::now();
                    if kind != FrameKind::Control {
                        bail!("unexpected frame from hub");
                    }
                    match wire::parse_control(&payload)? {
                        Control::Watch { active: true } => {
                            if subscription.is_none() {
                                match monitor.subscribe() {
                                    Ok(subscribed) => subscription = Some(subscribed),
                                    Err(SubscribeError::Stopped) => return Ok(()),
                                    Err(SubscribeError::TooManySubscribers) => {
                                        bail!("local viewer limit reached")
                                    }
                                }
                            }
                            encoder = SnapshotEncoder::default();
                            let initial = subscription.as_ref().unwrap().initial.clone();
                            send_snapshot(&mut writer, &mut encoder, initial, true).await?;
                        }
                        Control::Watch { active: false } => subscription = None,
                        Control::Resync => {
                            if subscription.is_some() {
                                send_snapshot(&mut writer, &mut encoder, monitor.snapshot(), true).await?;
                            }
                        }
                        Control::Ping => wire::write_control(&mut writer, &Control::Pong).await?,
                        _ => {}
                    }
                }
                event = async {
                    match subscription.as_mut() {
                        Some(subscription) => subscription.receiver.recv().await,
                        None => std::future::pending().await,
                    }
                } => match event {
                    Ok(SystemMonitorEvent::Snapshot(snapshot)) => {
                        send_snapshot(&mut writer, &mut encoder, snapshot, false).await?;
                    }
                    Ok(SystemMonitorEvent::Activity(activity)) => {
                        let data = serde_json::to_vec(&*activity)?;
                        wire::write_frame(&mut writer, FrameKind::Activity, &data).await?;
                    }
                    Err(RecvError::Lagged(_)) => {
                        send_snapshot(&mut writer, &mut encoder, monitor.snapshot(), true).await?;
                    }
                    Err(RecvError::Closed) => return Ok(()),
                },
                _ = ping.tick() => {
                    if monitor.stopped() {
                        return Ok(());
                    }
                    if last_frame.elapsed() > IDLE_TIMEOUT {
                        bail!("hub link idle for {IDLE_TIMEOUT:?}");
                    }
                    wire::write_control(&mut writer, &Control::Ping).await?;
                }
            }
        }
    }
    .await;
    reading.abort();
    result
}
