//! Voice control via WinRT `Windows.Media.SpeechRecognition` (offline list
//! grammar — no model downloads, runs on the system speech service).
//! Web Speech API isn't supported inside WebView2, so this lives in Rust.
//! A dedicated thread owns the recognizer + session; VOICE_ACTIVE starts it
//! and stopping is as simple as clearing the flag and letting the thread
//! unwind.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Once;
use std::time::Duration;
use tauri::{AppHandle, Wry};

static VOICE_ACTIVE: AtomicBool = AtomicBool::new(false);
static INIT: Once = Once::new();

pub fn set_enabled(app: &AppHandle<Wry>, enabled: bool) {
    VOICE_ACTIVE.store(enabled, Ordering::SeqCst);
    if enabled {
        start_thread(app.clone());
    }
}

fn start_thread(app: AppHandle<Wry>) {
    INIT.call_once(move || {
        std::thread::spawn(move || run(app));
    });
}

fn handle(app: &AppHandle<Wry>, cmd: &str) {
    match cmd {
        "peek" | "show" => crate::show_island(app),
        "hide" => crate::hide_island(app),
        "play" | "pause" | "next" | "previous" => {
            let app = app.clone();
            let cmd = cmd.to_string();
            tauri::async_runtime::spawn(async move {
                let _ = crate::with_access_token(&app, move |t| {
                    let cmd = cmd.clone();
                    async move { crate::spotify::control_playback(&t, &cmd).await }
                })
                .await;
            });
        }
        "like" => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let _ = crate::with_access_token(&app, |t| async move {
                    if let Ok(Some(p)) = crate::spotify::get_currently_playing(&t).await {
                        if let Some(item) = p.item {
                            let _ = crate::spotify::set_track_saved(&t, &item.id, true).await;
                        }
                    }
                    Ok(())
                })
                .await;
            });
        }
        _ => {}
    }
}

#[cfg(target_os = "windows")]
fn run(app: AppHandle<Wry>) {
    use windows::Foundation::TypedEventHandler;
    use windows::Media::SpeechRecognition::*;
    use windows_collections::IIterable;

    let recognizer = match SpeechRecognizer::new() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("voice: recognizer unavailable: {}", e);
            VOICE_ACTIVE.store(false, Ordering::SeqCst);
            return;
        }
    };

    let words: Vec<windows::core::HSTRING> = [
        "peek", "show", "hide", "play", "pause", "next", "previous", "like",
    ]
    .iter()
    .map(|s| windows::core::HSTRING::from(*s))
    .collect();
    let iterable: IIterable<windows::core::HSTRING> = words.into();

    let constraint = match SpeechRecognitionListConstraint::Create(&iterable) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("voice: constraint failed: {}", e);
            VOICE_ACTIVE.store(false, Ordering::SeqCst);
            return;
        }
    };
    if let Err(e) = recognizer
        .Constraints()
        .and_then(|c| c.Append(&constraint))
    {
        eprintln!("voice: append constraint failed: {}", e);
        VOICE_ACTIVE.store(false, Ordering::SeqCst);
        return;
    }
    if let Err(e) = recognizer
        .CompileConstraintsAsync()
        .and_then(|a| a.join())
    {
        eprintln!("voice: compile constraints failed: {}", e);
        VOICE_ACTIVE.store(false, Ordering::SeqCst);
        return;
    }

    let session = match recognizer.ContinuousRecognitionSession() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("voice: session failed: {}", e);
            VOICE_ACTIVE.store(false, Ordering::SeqCst);
            return;
        }
    };

    let app2 = app.clone();
    let handler = TypedEventHandler::<
        SpeechContinuousRecognitionSession,
        SpeechContinuousRecognitionResultGeneratedEventArgs,
    >::new(move |_s, args| {
        if let Some(args) = args.as_ref() {
            if let Ok(result) = args.Result() {
                let confident = result
                    .Confidence()
                    .map(|c| {
                        c == SpeechRecognitionConfidence::Medium
                            || c == SpeechRecognitionConfidence::High
                    })
                    .unwrap_or(false);
                if confident {
                    let text = result
                        .Text()
                        .map(|t| t.to_string_lossy().to_lowercase())
                        .unwrap_or_default();
                    handle(&app2, text.trim());
                }
            }
        }
        Ok(())
    });
    if session.ResultGenerated(&handler).is_err()
        || session.StartAsync().and_then(|a| a.join()).is_err()
    {
        eprintln!("voice: failed to start session");
        VOICE_ACTIVE.store(false, Ordering::SeqCst);
        return;
    }

    // Keep the session alive until disabled; objects drop with the thread.
    while VOICE_ACTIVE.load(Ordering::SeqCst) {
        std::thread::sleep(Duration::from_millis(400));
    }
    let _ = session.StopAsync().map(|a| a.join());
}

#[cfg(not(target_os = "windows"))]
fn run(_app: AppHandle<Wry>) {
    VOICE_ACTIVE.store(false, Ordering::SeqCst);
}
