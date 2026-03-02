use std::sync::mpsc;
use std::thread;

use tungstenite::{connect, Message};

use crate::shot_data::ShotData;

/// Spawn a background thread that connects to the flighthook WebSocket,
/// parses shot_result messages, and sends them on the returned receiver.
/// Automatically reconnects on failure with a 2-second delay.
pub fn spawn(url: &str) -> mpsc::Receiver<ShotData> {
    let (tx, rx) = mpsc::channel();
    let url = url.to_string();

    thread::spawn(move || {
        loop {
            match connect(&url) {
                Ok((mut ws, _)) => {
                    eprintln!("flighthook: connected to {url}");
                    // Send the required start handshake before the server will stream events.
                    let hello = r#"{"type":"start","name":"sample_range"}"#;
                    if let Err(e) = ws.send(tungstenite::Message::Text(hello.into())) {
                        eprintln!("flighthook: handshake send error: {e}");
                        break;
                    }
                    loop {
                        match ws.read() {
                            Ok(Message::Text(text)) => {
                                if let Some(shot) = parse_message(&text) {
                                    if tx.send(shot).is_err() {
                                        // Main thread has exited; stop.
                                        return;
                                    }
                                }
                            }
                            Ok(Message::Close(_)) => {
                                eprintln!("flighthook: connection closed");
                                break;
                            }
                            Ok(_) => {}
                            Err(e) => {
                                eprintln!("flighthook: read error: {e}");
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("flighthook: connect error: {e}");
                }
            }
            thread::sleep(std::time::Duration::from_secs(2));
        }
    });

    rx
}

fn parse_message(text: &str) -> Option<ShotData> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;

    let event = v.get("event")?;

    let kind = event.get("kind").and_then(|k| k.as_str()).unwrap_or("?");
    if kind != "launch_monitor" {
        // actor_status and similar high-frequency telemetry — no need to log
        return None;
    }

    let inner = event.get("event")?;
    let inner_type = inner.get("type").and_then(|t| t.as_str()).unwrap_or("?");
    if inner_type != "shot_result" {
        return None;
    }

    let shot = inner.get("shot")?;
    let timestamp = v.get("timestamp").and_then(|t| t.as_str()).unwrap_or("");
    match ShotData::from_flighthook(shot, timestamp) {
        Some(s) => {
            eprintln!("flighthook: shot carry={:.1}yds ball={:.1}mph", s.carry_yds, s.ball_speed_mph);
            Some(s)
        }
        None => {
            eprintln!("flighthook: shot_result parse failed — shot JSON: {shot}");
            None
        }
    }
}
