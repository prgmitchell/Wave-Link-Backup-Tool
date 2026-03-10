use crate::models::{ProbeWsResponse, WaveLinkAppInfo};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::Duration;
use tokio::runtime::Runtime;
use tokio::time::timeout;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::Message;
use url::Url;

pub fn probe_wave_link_ws(port: u16) -> ProbeWsResponse {
    let endpoint = format!("ws://127.0.0.1:{port}");
    let mut response = ProbeWsResponse {
        connected: false,
        endpoint: Some(endpoint.clone()),
        app_info: None,
        mixes: None,
        channels: None,
        errors: vec![],
    };

    let rt = match Runtime::new() {
        Ok(rt) => rt,
        Err(err) => {
            response
                .errors
                .push(format!("Failed to create runtime: {err}"));
            return response;
        }
    };

    let result = rt.block_on(async move {
        let url = Url::parse(&endpoint).map_err(|e| e.to_string())?;
        let mut req = url.into_client_request().map_err(|e| e.to_string())?;
        req.headers_mut()
            .insert("Origin", HeaderValue::from_static("streamdeck://"));

        let ws = timeout(Duration::from_millis(1_500), connect_async(req))
            .await
            .map_err(|_| "Connection timed out".to_string())?
            .map_err(|e| e.to_string())?;

        let (mut write, mut read) = ws.0.split();

        let reqs = vec![
            json!({ "jsonrpc": "2.0", "method": "getApplicationInfo", "id": 1 }),
            json!({ "jsonrpc": "2.0", "method": "getMixes", "id": 2 }),
            json!({ "jsonrpc": "2.0", "method": "getChannels", "id": 3 }),
        ];

        for req in reqs {
            write
                .send(Message::Text(req.to_string()))
                .await
                .map_err(|e| e.to_string())?;
        }

        let mut payloads: HashMap<i64, Value> = HashMap::new();
        for _ in 0..12 {
            let maybe = timeout(Duration::from_millis(400), read.next()).await;
            let Some(Ok(msg)) = maybe.ok().flatten() else {
                continue;
            };
            if let Message::Text(text) = msg {
                if let Ok(value) = serde_json::from_str::<Value>(&text) {
                    if let Some(id) = value.get("id").and_then(|v| v.as_i64()) {
                        payloads.insert(id, value);
                    }
                }
            }
            if payloads.contains_key(&1) && payloads.contains_key(&2) && payloads.contains_key(&3) {
                break;
            }
        }

        Ok::<HashMap<i64, Value>, String>(payloads)
    });

    match result {
        Ok(payloads) => {
            response.connected = true;
            response.app_info = payloads.get(&1).cloned();
            response.mixes = payloads.get(&2).cloned();
            response.channels = payloads.get(&3).cloned();
        }
        Err(err) => response.errors.push(err),
    }

    response
}

pub fn app_info_from_probe(value: Option<&Value>) -> Option<WaveLinkAppInfo> {
    let result = value?.get("result")?;
    Some(WaveLinkAppInfo {
        name: result
            .get("name")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        app_id: result
            .get("appID")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        version: result
            .get("version")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        build: result.get("build").and_then(|v| v.as_i64()),
        interface_revision: result.get("interfaceRevision").and_then(|v| v.as_i64()),
    })
}

pub fn apply_channel_levels(port: u16, snapshot: &Value) -> Result<usize, String> {
    let channels = snapshot
        .get("result")
        .and_then(|v| v.get("channels"))
        .or_else(|| snapshot.get("channels"))
        .and_then(|v| v.as_array())
        .ok_or("Invalid live channels snapshot format")?;

    let mut level_updates = Vec::new();
    for channel in channels {
        let Some(id) = channel.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(level) = channel.get("level").and_then(|v| v.as_f64()) else {
            continue;
        };
        level_updates.push((id.to_string(), level));
    }

    if level_updates.is_empty() {
        return Ok(0);
    }

    let endpoint = format!("ws://127.0.0.1:{port}");
    let rt = Runtime::new().map_err(|e| e.to_string())?;
    rt.block_on(async move {
        let url = Url::parse(&endpoint).map_err(|e| e.to_string())?;
        let mut req = url.into_client_request().map_err(|e| e.to_string())?;
        req.headers_mut()
            .insert("Origin", HeaderValue::from_static("streamdeck://"));

        let ws = timeout(Duration::from_millis(1_500), connect_async(req))
            .await
            .map_err(|_| "Connection timed out".to_string())?
            .map_err(|e| e.to_string())?;
        let (mut write, _read) = ws.0.split();

        for (idx, (id, level)) in level_updates.iter().enumerate() {
            let req = json!({
                "jsonrpc": "2.0",
                "method": "setChannel",
                "id": 4000 + idx as i64,
                "params": {
                    "id": id,
                    "level": level
                }
            });
            write
                .send(Message::Text(req.to_string()))
                .await
                .map_err(|e| e.to_string())?;
        }
        Ok::<usize, String>(level_updates.len())
    })
}
