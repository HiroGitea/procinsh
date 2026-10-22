//! A hub and an agent in one process, linked over loopback TCP.

use super::{AppState, hosts};
use axum::{
    body::{Body, HttpBody},
    http::{Request, StatusCode},
};
use serde_json::Value;
use std::{sync::Arc, time::Duration};
use tokio::{net::TcpListener, sync::watch, time::timeout};
use tower::ServiceExt;

fn info(name: &str) -> hosts::HostInfo {
    hosts::HostInfo {
        name: name.into(),
        ..hosts::HostInfo::local(None)
    }
}

fn request(path: &str) -> Request<Body> {
    Request::builder()
        .uri(path)
        .header("host", "127.0.0.1:8080")
        .body(Body::empty())
        .unwrap()
}

async fn until(what: &str, mut condition: impl FnMut() -> bool) {
    timeout(Duration::from_secs(20), async {
        while !condition() {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting for {what}"));
}

/// The next SSE event as `(name, data)`, skipping keep-alive comments.
async fn next_event(body: &mut Body) -> (String, Value) {
    loop {
        let frame = timeout(
            Duration::from_secs(20),
            std::future::poll_fn(|cx| std::pin::Pin::new(&mut *body).poll_frame(cx)),
        )
        .await
        .expect("SSE event timed out")
        .unwrap()
        .unwrap()
        .into_data()
        .unwrap();
        let text = std::str::from_utf8(&frame).unwrap();
        let name = text.lines().find_map(|l| l.strip_prefix("event: "));
        let data = text.lines().find_map(|l| l.strip_prefix("data: "));
        if let (Some(name), Some(data)) = (name, data) {
            return (name.into(), serde_json::from_str(data).unwrap());
        }
    }
}

async fn json(app: &axum::Router, path: &str) -> (StatusCode, Value) {
    let response = app.clone().oneshot(request(path)).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn agents_relay_snapshots_while_watched_and_go_offline() {
    let interval = Duration::from_secs(1);
    let hub = Arc::new(AppState::with_host(interval, info("hub")));
    let agent = Arc::new(AppState::with_host(interval, info("agent-a")));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (shutdown, _) = watch::channel(false);
    let token: Option<Arc<str>> = Some("secret".into());
    let hub_task = tokio::spawn(hosts::run_agent_listener(
        listener,
        hub.hosts.clone(),
        token.clone(),
        shutdown.subscribe(),
    ));
    let (agent_shutdown, _) = watch::channel(false);
    let agent_task = tokio::spawn(hosts::run_hub_connector(
        address.to_string(),
        info("agent-a"),
        token,
        agent.system_monitor.clone(),
        agent_shutdown.subscribe(),
    ));
    until("agent admission", || hub.hosts.get("agent-a").is_some()).await;
    let app = super::router::router(hub.clone(), "127.0.0.1:8080".parse().unwrap());
    let (status, hosts) = json(&app, "/api/hosts").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(hosts["local"], "hub");
    assert_eq!(hosts["hosts"][0]["local"], true);
    assert_eq!(hosts["hosts"][1]["name"], "agent-a");
    assert_eq!(hosts["hosts"][1]["status"]["state"], "online");
    assert!(
        !agent.system_monitor.active(),
        "agents idle without viewers"
    );
    let (status, error) = json(&app, "/api/system/events?host=missing").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(error["error"], "Unknown host");

    let response = app
        .clone()
        .oneshot(request("/api/system/events?host=agent-a"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let mut body = response.into_body();
    let (event, initial) = next_event(&mut body).await;
    assert_eq!(
        (event.as_str(), &initial["kind"]),
        ("snapshot", &Value::from("full"))
    );
    // The agent starts collecting for the viewer and relays a full snapshot, then deltas.
    let mut sequence = initial["sequence"].as_u64().unwrap();
    let mut saw = (false, false, false);
    while !(saw.0 && saw.1 && saw.2) {
        let (event, data) = next_event(&mut body).await;
        match event.as_str() {
            "snapshot" => {
                let next = data["sequence"].as_u64().unwrap();
                assert!(next > sequence);
                if data["kind"] == "delta" {
                    assert_eq!(data["base_sequence"].as_u64(), Some(sequence));
                    saw.1 = true;
                }
                // Every payload lists all processes; the first collection may arrive as a delta.
                if data["processes"].as_array().is_some_and(|p| !p.is_empty()) {
                    assert!(data["rdma_ports"].is_array());
                    saw.0 = true;
                }
                sequence = next;
            }
            "activity" => {
                assert!(data["status"].is_object());
                saw.2 = true;
            }
            other => panic!("unexpected event {other}"),
        }
    }
    assert!(agent.system_monitor.active());
    let (_, hosts) = json(&app, "/api/hosts").await;
    assert!(hosts["hosts"][1]["captured_at"].is_u64());
    drop(body);
    until("agent idle", || !agent.system_monitor.active()).await;

    // The merged view lists hosts, tags every payload with its host, and reports links.
    let response = app
        .clone()
        .oneshot(request("/api/system/events?host=all"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let mut body = response.into_body();
    let (event, hosts) = next_event(&mut body).await;
    assert_eq!(event, "hosts");
    assert_eq!(hosts["hosts"].as_array().unwrap().len(), 2);
    let mut seen = std::collections::BTreeSet::new();
    while !(seen.contains("links")
        && seen.contains("snapshot:hub")
        && seen.contains("snapshot:agent-a")
        && seen.contains("activity:agent-a"))
    {
        let (event, data) = next_event(&mut body).await;
        match event.as_str() {
            "links" => {
                assert!(data["sockets"].is_array() && data["rdma"].is_array());
                seen.insert("links".to_string());
            }
            "host" => assert_eq!(data["host"], "agent-a"),
            "snapshot" | "activity" => {
                let host = data["host"].as_str().unwrap().to_owned();
                if event == "activity"
                    || data["processes"].as_array().is_some_and(|p| !p.is_empty())
                {
                    seen.insert(format!("{event}:{host}"));
                }
            }
            other => panic!("unexpected merged event {other}"),
        }
    }
    assert!(agent.system_monitor.active() && hub.system_monitor.active());
    drop(body);
    until("merged viewers released", || {
        !agent.system_monitor.active() && !hub.system_monitor.active()
    })
    .await;

    // A wrong token is rejected before admission.
    let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let hello = br#"{"type":"hello","protocol":1,"host":"intruder","machine_id":"x","version":"0","kernel":null,"token":"wrong"}"#;
    stream.write_u32(hello.len() as u32 + 1).await.unwrap();
    stream.write_u8(1).await.unwrap();
    stream.write_all(hello).await.unwrap();
    let length = stream.read_u32().await.unwrap() as usize;
    let mut reply = vec![0; length];
    stream.read_exact(&mut reply).await.unwrap();
    let reply: Value = serde_json::from_slice(&reply[1..]).unwrap();
    assert_eq!(reply["type"], "reject");
    assert!(hub.hosts.get("intruder").is_none());

    // A viewer of an offline host keeps its subscription and sees the transition.
    let response = app
        .clone()
        .oneshot(request("/api/system/events?host=agent-a"))
        .await
        .unwrap();
    let mut body = response.into_body();
    next_event(&mut body).await;
    agent_shutdown.send_replace(true);
    agent_task.await.unwrap();
    loop {
        let (event, data) = next_event(&mut body).await;
        if event == "host" {
            assert_eq!(data["status"]["state"], "offline");
            break;
        }
    }
    let (_, hosts) = json(&app, "/api/hosts").await;
    assert_eq!(hosts["hosts"][1]["status"]["state"], "offline");
    drop(body);
    shutdown.send_replace(true);
    hub_task.await.unwrap();
    for state in [&hub, &agent] {
        state.stop();
        state.join_collectors().unwrap();
    }
}
