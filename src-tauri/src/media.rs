//! Windows System Media Transport Controls (GSMTC) watcher.
//!
//! Polls the OS media-session manager locally every ~700ms — far cheaper than
//! an HTTPS round-trip — and emits `smtc-change` the instant Spotify's track
//! or play-state flips. The webview treats that event as a wake-up trigger to
//! refetch, so "music started → island pops" and track-change pops happen
//! immediately instead of waiting on the next scheduled API poll. The Spotify
//! Web API stays the source of truth for everything (art, queue, devices);
//! SMTC is only the doorbell.

use tauri::{AppHandle, Emitter, Wry};

#[cfg(target_os = "windows")]
pub fn start(app: AppHandle<Wry>) {
    std::thread::spawn(move || run(app));
}

#[cfg(not(target_os = "windows"))]
pub fn start(_app: AppHandle<Wry>) {}

#[cfg(target_os = "windows")]
fn run(app: AppHandle<Wry>) {
    use std::time::Duration;
    use windows::Media::Control::{
        GlobalSystemMediaTransportControlsSessionManager as Manager,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
    };

    let mgr = match Manager::RequestAsync().and_then(|op| op.join()) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("smtc: session manager unavailable: {}", e);
            return;
        }
    };

    let mut last_sig = String::new();
    loop {
        std::thread::sleep(Duration::from_millis(700));

        let sessions = match mgr.GetSessions() {
            Ok(s) => s,
            Err(_) => continue,
        };

        // Find the Spotify session (SourceAppUserModelId contains "spotify").
        let mut found: Option<(bool, String)> = None;
        for i in 0..sessions.Size().unwrap_or(0) {
            let Ok(session) = sessions.GetAt(i) else { continue };
            let Ok(app_id) = session.SourceAppUserModelId() else { continue };
            if !app_id.to_string_lossy().to_lowercase().contains("spotify") {
                continue;
            }
            let playing = session
                .GetPlaybackInfo()
                .and_then(|p| p.PlaybackStatus())
                .map(|s| s == Status::Playing)
                .unwrap_or(false);
            let title = session
                .TryGetMediaPropertiesAsync()
                .and_then(|op| op.join())
                .and_then(|p| p.Title())
                .map(|t| t.to_string_lossy())
                .unwrap_or_default();
            found = Some((playing, title));
            break;
        }

        let sig = match &found {
            Some((playing, title)) => format!("{}|{}", playing, title),
            None => String::from("none"),
        };
        if sig != last_sig {
            last_sig = sig;
            let payload = found.map(|(playing, title)| {
                serde_json::json!({ "playing": playing, "title": title })
            });
            let _ = app.emit("smtc-change", payload);
        }
    }
}
