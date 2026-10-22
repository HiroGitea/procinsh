use super::super::{
    AppState,
    hosts::{self, FeedEvent, FeedSubscribeError, HostStatus},
    system::{SnapshotEncoder, SubscribeError, Subscription, SystemMonitorEvent},
};
use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::{
        IntoResponse, Response, Sse,
        sse::{Event, KeepAlive},
    },
};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashSet;
use std::{
    convert::Infallible,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{sync::mpsc, task::JoinSet};

#[derive(Deserialize)]
pub(super) struct EventsQuery {
    /// A host name; absent or the local name selects this machine.
    host: Option<String>,
}

pub(super) async fn events(
    State(s): State<Arc<AppState>>,
    Query(query): Query<EventsQuery>,
) -> Result<Response, StatusCode> {
    match query.host {
        Some(host) if host == "all" => merged_events(s).await,
        Some(host) if host != s.hosts.local().name => match s.hosts.get(&host) {
            Some(_) => remote_events(s, host).await,
            None => Ok((
                StatusCode::NOT_FOUND,
                Json(json!({"error": "Unknown host"})),
            )
                .into_response()),
        },
        _ => local_events(s).await,
    }
}

/// Relays a remote host's merged snapshots, activity and link status.
async fn remote_events(s: Arc<AppState>, host: String) -> Result<Response, StatusCode> {
    if s.system_monitor.stopped() {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }
    let feed = s.hosts.get(&host).ok_or(StatusCode::NOT_FOUND)?;
    let mut subscription = feed.subscribe().map_err(|error| match error {
        FeedSubscribeError::TooManySubscribers => StatusCode::TOO_MANY_REQUESTS,
    })?;
    let stream = async_stream::stream! {
        let mut last = subscription.initial_sequence;
        yield Ok::<_, Infallible>(Event::default().event("snapshot").data(subscription.initial.clone()));
        if matches!(subscription.status, HostStatus::Offline { .. }) {
            yield Ok(Event::default().event("host").data(json!({"status": subscription.status}).to_string()));
        }
        loop {
            if s.system_monitor.stopped() { break; }
            match tokio::time::timeout(Duration::from_secs(1), subscription.receiver.recv()).await {
                Ok(Ok(FeedEvent::Snapshot { sequence, data })) => {
                    // A gap recovery snapshot can already include older relayed frames.
                    if sequence > last {
                        last = sequence;
                        yield Ok(Event::default().event("snapshot").data(&*data));
                    }
                }
                Ok(Ok(FeedEvent::Activity(data))) => yield Ok(Event::default().event("activity").data(&*data)),
                Ok(Ok(FeedEvent::Status(status))) => {
                    yield Ok(Event::default().event("host").data(json!({"status": status}).to_string()));
                }
                Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(n))) => {
                    yield Ok(Event::default().event("gap").data(json!({"dropped_frames": n}).to_string()));
                    let (sequence, full) = feed.full();
                    last = sequence;
                    yield Ok(Event::default().event("snapshot").data(full));
                }
                Ok(Err(_)) => break,
                Err(_) => {}
            }
        }
    };
    Ok(Sse::new(stream)
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(10)))
        .into_response())
}

async fn local_events(s: Arc<AppState>) -> Result<Response, StatusCode> {
    let mut subscription = s.system_monitor.subscribe().map_err(|error| match error {
        SubscribeError::Stopped => StatusCode::SERVICE_UNAVAILABLE,
        SubscribeError::TooManySubscribers => StatusCode::TOO_MANY_REQUESTS,
    })?;
    let stream = async_stream::stream! {
        let mut delivery = SnapshotEncoder::default();
        let initial = delivery.encode(subscription.initial.clone(), true, Instant::now()).unwrap();
        log::debug!("SSE /api/system/events event=snapshot payload_bytes={}", initial.len());
        yield Ok::<_,Infallible>(Event::default().event("snapshot").data(initial));
        loop{if s.system_monitor.stopped(){break;}
            match tokio::time::timeout(Duration::from_secs(1),subscription.receiver.recv()).await {
                Ok(Ok(message)) => {
                    let (event, data) = match message {
                        SystemMonitorEvent::Snapshot(data) => ("snapshot", delivery.encode(data, false, Instant::now()).unwrap()),
                        SystemMonitorEvent::Activity(data) => ("activity", serde_json::to_string(&*data).unwrap()),
                    };
                    log::debug!("SSE /api/system/events event={event} payload_bytes={}", data.len());
                    yield Ok(Event::default().event(event).data(data));
                },
                Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(n))) => {
                    let data = json!({"dropped_frames":n}).to_string();
                    log::debug!("SSE /api/system/events event=gap payload_bytes={} dropped_frames={n}", data.len());
                    yield Ok(Event::default().event("gap").data(data));
                    let snapshot = delivery.encode(s.system_monitor.snapshot(), true, Instant::now()).unwrap();
                    log::debug!("SSE /api/system/events event=snapshot payload_bytes={}", snapshot.len());
                    yield Ok(Event::default().event("snapshot").data(snapshot));
                },
                Ok(Err(_))=>break,Err(_)=>{}
            }
        }
    };
    Ok(Sse::new(stream)
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(10)))
        .into_response())
}

/// Adds `"host"` to a JSON object payload.
fn with_host(host: &str, data: &str) -> String {
    let host = serde_json::to_string(host).unwrap();
    match data.strip_prefix('{') {
        Some(rest) if rest.trim_start().starts_with('}') => format!("{{\"host\":{host}}}"),
        Some(rest) => format!("{{\"host\":{host},{rest}"),
        None => data.to_owned(),
    }
}

type Outgoing = mpsc::Sender<(&'static str, String)>;

/// Forwards the local host with a per-connection encoder, like the local stream.
async fn forward_local(
    s: Arc<AppState>,
    mut subscription: Subscription,
    host: String,
    out: Outgoing,
) {
    let mut encoder = SnapshotEncoder::default();
    let initial = encoder
        .encode(subscription.initial.clone(), true, Instant::now())
        .unwrap();
    if out
        .send(("snapshot", with_host(&host, &initial)))
        .await
        .is_err()
    {
        return;
    }
    while !s.system_monitor.stopped() {
        let messages = match tokio::time::timeout(
            Duration::from_secs(1),
            subscription.receiver.recv(),
        )
        .await
        {
            Ok(Ok(SystemMonitorEvent::Snapshot(data))) => {
                vec![(
                    "snapshot",
                    encoder.encode(data, false, Instant::now()).unwrap(),
                )]
            }
            Ok(Ok(SystemMonitorEvent::Activity(data))) => {
                vec![("activity", serde_json::to_string(&*data).unwrap())]
            }
            Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(n))) => vec![
                ("gap", json!({"dropped_frames": n}).to_string()),
                (
                    "snapshot",
                    encoder
                        .encode(s.system_monitor.snapshot(), true, Instant::now())
                        .unwrap(),
                ),
            ],
            Ok(Err(_)) => return,
            Err(_) => continue,
        };
        for (event, data) in messages {
            if out.send((event, with_host(&host, &data))).await.is_err() {
                return;
            }
        }
    }
}

/// Forwards a remote host's relayed events and status.
async fn forward_remote(
    feed: Arc<hosts::HostFeed>,
    mut subscription: hosts::FeedSubscription,
    host: String,
    out: Outgoing,
) {
    let mut last = subscription.initial_sequence;
    let mut messages = vec![("snapshot", subscription.initial.clone())];
    messages.push(("host", json!({"status": subscription.status}).to_string()));
    loop {
        for (event, data) in messages.drain(..) {
            if out.send((event, with_host(&host, &data))).await.is_err() {
                return;
            }
        }
        match subscription.receiver.recv().await {
            Ok(FeedEvent::Snapshot { sequence, data }) if sequence > last => {
                last = sequence;
                messages.push(("snapshot", data.to_string()));
            }
            Ok(FeedEvent::Snapshot { .. }) => {}
            Ok(FeedEvent::Activity(data)) => messages.push(("activity", data.to_string())),
            Ok(FeedEvent::Status(status)) => {
                messages.push(("host", json!({"status": status}).to_string()));
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                let (sequence, full) = feed.full();
                last = sequence;
                messages.push(("gap", json!({"dropped_frames": n}).to_string()));
                messages.push(("snapshot", full));
            }
            Err(_) => return,
        }
    }
}

enum MergedStep {
    Message(Option<(&'static str, String)>),
    Hosts,
    Tick,
}

/// Every host on one stream. Payloads carry `"host"`; `hosts` lists the hosts and
/// `links` the connections between them. Watching the merged view activates every agent.
async fn merged_events(s: Arc<AppState>) -> Result<Response, StatusCode> {
    let local = s.system_monitor.subscribe().map_err(|error| match error {
        SubscribeError::Stopped => StatusCode::SERVICE_UNAVAILABLE,
        SubscribeError::TooManySubscribers => StatusCode::TOO_MANY_REQUESTS,
    })?;
    let local_name = s.hosts.local().name.clone();
    let (out, mut incoming) = mpsc::channel(64);
    // Dropping the stream drops the set, which aborts every forwarder and its subscription.
    let mut forwarders = JoinSet::new();
    forwarders.spawn(forward_local(
        s.clone(),
        local,
        local_name.clone(),
        out.clone(),
    ));
    let mut changes = s.hosts.changes();
    let stream = async_stream::stream! {
        let mut forwarded = HashSet::new();
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        let mut links = String::new();
        let mut step = MergedStep::Hosts;
        loop {
            if s.system_monitor.stopped() { break; }
            match step {
                MergedStep::Message(Some((event, data))) => {
                    yield Ok::<_, Infallible>(Event::default().event(event).data(data));
                }
                MergedStep::Message(None) => break,
                MergedStep::Hosts => {
                    changes.mark_unchanged();
                    for (name, feed) in s.hosts.feeds() {
                        if forwarded.contains(&name) { continue; }
                        match feed.subscribe() {
                            Ok(subscription) => {
                                forwarded.insert(name.clone());
                                forwarders.spawn(forward_remote(feed, subscription, name, out.clone()));
                            }
                            Err(FeedSubscribeError::TooManySubscribers) => {
                                log::warn!("merged view skips host {name}: viewer limit reached");
                            }
                        }
                    }
                    let hosts = json!({
                        "local": local_name,
                        "hosts": s.hosts.summaries(s.system_monitor.captured_at()),
                    });
                    yield Ok(Event::default().event("hosts").data(hosts.to_string()));
                }
                MergedStep::Tick => {
                    let mut snapshots = vec![(local_name.clone(), s.system_monitor.snapshot().relations_json())];
                    for (name, feed) in s.hosts.feeds() {
                        if let Some(snapshot) = feed.snapshot() {
                            snapshots.push((name, snapshot));
                        }
                    }
                    let current = serde_json::to_string(&hosts::correlate(&snapshots)).unwrap();
                    if current != links {
                        links = current;
                        yield Ok(Event::default().event("links").data(links.clone()));
                    }
                }
            }
            step = tokio::select! {
                message = incoming.recv() => MergedStep::Message(message),
                Ok(()) = changes.changed() => MergedStep::Hosts,
                _ = tick.tick() => MergedStep::Tick,
            };
        }
        drop(forwarders);
    };
    Ok(Sse::new(stream)
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(10)))
        .into_response())
}

#[cfg(test)]
mod tests {
    #[test]
    fn host_is_added_to_object_payloads() {
        assert_eq!(super::with_host("a", r#"{"x":1}"#), r#"{"host":"a","x":1}"#);
        assert_eq!(super::with_host("a", "{}"), r#"{"host":"a"}"#);
        assert_eq!(super::with_host("a\"b", "{}"), r#"{"host":"a\"b"}"#);
    }
}
