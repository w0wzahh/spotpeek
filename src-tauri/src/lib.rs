mod auth;
mod spotify;

use serde::{Deserialize, Serialize};
use tauri::{
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, Wry,
};
use tauri_plugin_positioner::{Position, WindowExt};
use tauri_plugin_store::StoreExt;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SpotifyTokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<u64>,
}

fn load_tokens(app: &tauri::AppHandle<Wry>) -> Result<Option<SpotifyTokens>, String> {
    let store = app.store("store.json").map_err(|e| e.to_string())?;
    match store.get("spotify_tokens") {
        Some(val) => {
            let tokens: SpotifyTokens = serde_json::from_value(val).map_err(|e| e.to_string())?;
            Ok(Some(tokens))
        }
        None => Ok(None),
    }
}

fn save_tokens(app: &tauri::AppHandle<Wry>, tokens: &SpotifyTokens) -> Result<(), String> {
    let store = app.store("store.json").map_err(|e| e.to_string())?;
    store.set("spotify_tokens", serde_json::to_value(tokens).map_err(|e| e.to_string())?);
    store.save().map_err(|e| e.to_string())?;
    Ok(())
}

fn clear_tokens(app: &tauri::AppHandle<Wry>) -> Result<(), String> {
    let store = app.store("store.json").map_err(|e| e.to_string())?;
    store.delete("spotify_tokens");
    store.save().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn start_spotify_login(app: tauri::AppHandle<Wry>) -> Result<String, String> {
    let store = app.store("store.json").map_err(|e| e.to_string())?;
    let client_id = store
        .get("client_id")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .ok_or_else(|| "Spotify Client ID not configured. Please set it in settings.".to_string())?;

    let flow = auth::build_auth_url(
        &client_id,
        &[
            "user-read-currently-playing",
            "user-read-playback-state",
            "user-modify-playback-state",
        ],
    );

    let app_handle = app.clone();
    tokio::spawn(async move {
        let result = auth::start_callback_server(flow.csrf_token.clone(), client_id.clone(), flow.pkce_verifier.clone()).await;
        match result {
            Ok((token_response, _)) => {
                let tokens = SpotifyTokens {
                    access_token: token_response.access_token.clone(),
                    refresh_token: token_response.refresh_token.clone(),
                    expires_at: Some(
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs()
                        + token_response.expires_in,
                    ),
                };
                if let Err(e) = save_tokens(&app_handle, &tokens) {
                    eprintln!("Failed to save tokens: {}", e);
                }
            }
            Err(e) => eprintln!("Auth error: {}", e),
        }
    });

    Ok(flow.auth_url)
}

#[tauri::command]
async fn get_spotify_tokens(app: tauri::AppHandle<Wry>) -> Result<Option<SpotifyTokens>, String> {
    load_tokens(&app)
}

#[tauri::command]
async fn logout_spotify(app: tauri::AppHandle<Wry>) -> Result<(), String> {
    clear_tokens(&app)
}

#[tauri::command]
async fn get_client_id(app: tauri::AppHandle<Wry>) -> Result<Option<String>, String> {
    let store = app.store("store.json").map_err(|e| e.to_string())?;
    Ok(store.get("client_id").and_then(|v| v.as_str().map(|s| s.to_string())))
}

#[tauri::command]
async fn set_client_id(app: tauri::AppHandle<Wry>, client_id: String) -> Result<(), String> {
    let store = app.store("store.json").map_err(|e| e.to_string())?;
    store.set("client_id", serde_json::json!(client_id));
    store.save().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn get_current_playback(app: tauri::AppHandle<Wry>) -> Result<Option<spotify::PlaybackState>, String> {
    let tokens_opt = load_tokens(&app)?;

    let tokens = match tokens_opt {
        Some(t) => t,
        None => return Ok(None),
    };

    match spotify::get_currently_playing(&tokens.access_token).await {
        Ok(state) => Ok(state),
        Err(e) if e.contains("401") => {
            if let Some(refresh) = &tokens.refresh_token {
                let store = app.store("store.json").map_err(|e| e.to_string())?;
                let client_id = store
                    .get("client_id")
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
                    .ok_or_else(|| "Client ID missing".to_string())?;
                match auth::refresh_access_token(refresh, &client_id).await {
                    Ok(new_token) => {
                        let new_tokens = SpotifyTokens {
                            access_token: new_token.access_token.clone(),
                            refresh_token: new_token.refresh_token.or_else(|| Some(refresh.clone())),
                            expires_at: Some(
                                std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap()
                                    .as_secs()
                                + new_token.expires_in,
                            ),
                        };
                        save_tokens(&app, &new_tokens)?;
                        spotify::get_currently_playing(&new_tokens.access_token)
                            .await
                            .map_err(|e| e.to_string())
                    }
                    Err(e) => Err(e),
                }
            } else {
                Err("Session expired. Please log in again.".to_string())
            }
        }
        Err(e) => Err(e),
    }
}

#[tauri::command]
async fn control_playback(
    app: tauri::AppHandle<Wry>,
    action: String,
) -> Result<(), String> {
    let tokens_opt = load_tokens(&app)?;

    let tokens = match tokens_opt {
        Some(t) => t,
        None => return Err("Not authenticated".to_string()),
    };

    match spotify::control_playback(&tokens.access_token, &action).await {
        Ok(()) => Ok(()),
        Err(e) if e.contains("401") => {
            if let Some(refresh) = &tokens.refresh_token {
                let store = app.store("store.json").map_err(|e| e.to_string())?;
                let client_id = store
                    .get("client_id")
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
                    .ok_or_else(|| "Client ID missing".to_string())?;
                match auth::refresh_access_token(refresh, &client_id).await {
                    Ok(new_token) => {
                        let new_tokens = SpotifyTokens {
                            access_token: new_token.access_token.clone(),
                            refresh_token: new_token.refresh_token.or_else(|| Some(refresh.clone())),
                            expires_at: Some(
                                std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap()
                                    .as_secs()
                                + new_token.expires_in,
                            ),
                        };
                        save_tokens(&app, &new_tokens)?;
                        spotify::control_playback(&new_tokens.access_token, &action)
                            .await
                            .map_err(|e| e.to_string())
                    }
                    Err(e) => Err(e),
                }
            } else {
                Err("Session expired. Please log in again.".to_string())
            }
        }
        Err(e) => Err(e),
    }
}

#[tauri::command]
async fn seek_playback(
    app: tauri::AppHandle<Wry>,
    position_ms: u64,
) -> Result<(), String> {
    let tokens_opt = load_tokens(&app)?;
    let tokens = match tokens_opt {
        Some(t) => t,
        None => return Err("Not authenticated".to_string()),
    };

    match spotify::seek_playback(&tokens.access_token, position_ms).await {
        Ok(()) => Ok(()),
        Err(e) if e.contains("401") => {
            if let Some(refresh) = &tokens.refresh_token {
                let store = app.store("store.json").map_err(|e| e.to_string())?;
                let client_id = store
                    .get("client_id")
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
                    .ok_or_else(|| "Client ID missing".to_string())?;
                match auth::refresh_access_token(refresh, &client_id).await {
                    Ok(new_token) => {
                        let new_tokens = SpotifyTokens {
                            access_token: new_token.access_token.clone(),
                            refresh_token: new_token.refresh_token.or_else(|| Some(refresh.clone())),
                            expires_at: Some(
                                std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap()
                                    .as_secs()
                                + new_token.expires_in,
                            ),
                        };
                        save_tokens(&app, &new_tokens)?;
                        spotify::seek_playback(&new_tokens.access_token, position_ms)
                            .await
                            .map_err(|e| e.to_string())
                    }
                    Err(e) => Err(e),
                }
            } else {
                Err("Session expired. Please log in again.".to_string())
            }
        }
        Err(e) => Err(e),
    }
}

#[tauri::command]
async fn set_volume(
    app: tauri::AppHandle<Wry>,
    volume_percent: u8,
) -> Result<(), String> {
    let tokens_opt = load_tokens(&app)?;
    let tokens = match tokens_opt {
        Some(t) => t,
        None => return Err("Not authenticated".to_string()),
    };

    match spotify::set_volume(&tokens.access_token, volume_percent).await {
        Ok(()) => Ok(()),
        Err(e) if e.contains("401") => {
            if let Some(refresh) = &tokens.refresh_token {
                let store = app.store("store.json").map_err(|e| e.to_string())?;
                let client_id = store
                    .get("client_id")
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
                    .ok_or_else(|| "Client ID missing".to_string())?;
                match auth::refresh_access_token(refresh, &client_id).await {
                    Ok(new_token) => {
                        let new_tokens = SpotifyTokens {
                            access_token: new_token.access_token.clone(),
                            refresh_token: new_token.refresh_token.or_else(|| Some(refresh.clone())),
                            expires_at: Some(
                                std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap()
                                    .as_secs()
                                + new_token.expires_in,
                            ),
                        };
                        save_tokens(&app, &new_tokens)?;
                        spotify::set_volume(&new_tokens.access_token, volume_percent)
                            .await
                            .map_err(|e| e.to_string())
                    }
                    Err(e) => Err(e),
                }
            } else {
                Err("Session expired. Please log in again.".to_string())
            }
        }
        Err(e) => Err(e),
    }
}

#[tauri::command]
async fn show_window(app: tauri::AppHandle<Wry>) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
    Ok(())
}

#[tauri::command]
async fn hide_window(app: tauri::AppHandle<Wry>) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
    Ok(())
}

#[tauri::command]
async fn toggle_window(app: tauri::AppHandle<Wry>) -> Result<bool, String> {
    if let Some(window) = app.get_webview_window("main") {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
            Ok(false)
        } else {
            let _ = window.show();
            let _ = window.set_focus();
            Ok(true)
        }
    } else {
        Ok(false)
    }
}

fn create_app_icon() -> tauri::image::Image<'static> {
    let w = 128u32;
    let h = 128u32;
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    let cx = w as f32 / 2.0;
    let cy = h as f32 / 2.0;
    let radius = 54.0;

    // Green circle with 1px anti-aliased edge
    for y in 0..h {
        for x in 0..w {
            let idx = ((y * w + x) * 4) as usize;
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist < radius {
                rgba[idx] = 29;
                rgba[idx + 1] = 185;
                rgba[idx + 2] = 84;
                rgba[idx + 3] = 255;
            } else if dist < radius + 1.5 {
                let alpha = 255.0 * (1.0 - (dist - radius) / 1.5);
                rgba[idx] = 29;
                rgba[idx + 1] = 185;
                rgba[idx + 2] = 84;
                rgba[idx + 3] = alpha as u8;
            }
        }
    }

    // White play triangle (barycentric)
    let ax = 56.0; let ay = 46.0;
    let bx = 56.0; let by = 82.0;
    let cx_t = 88.0; let cy_t = 64.0;
    let denom = (by - cy_t) * (ax - cx_t) + (cx_t - bx) * (ay - cy_t);

    for y in 0..h {
        for x in 0..w {
            let px = x as f32;
            let py = y as f32;
            let a = ((by - cy_t) * (px - cx_t) + (cx_t - bx) * (py - cy_t)) / denom;
            let b = ((cy_t - ay) * (px - cx_t) + (ax - cx_t) * (py - cy_t)) / denom;
            let c = 1.0 - a - b;
            if a >= -0.02 && b >= -0.02 && c >= -0.02 {
                let idx = ((y * w + x) * 4) as usize;
                if rgba[idx + 3] > 0 {
                    rgba[idx] = 255;
                    rgba[idx + 1] = 255;
                    rgba[idx + 2] = 255;
                    rgba[idx + 3] = 255;
                }
            }
        }
    }

    tauri::image::Image::new_owned(rgba, w, h)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_positioner::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .setup(|app| {
            // Tray menu
            let show_item = tauri::menu::MenuItemBuilder::with_id("show", "Show").build(app)?;
            let quit_item = tauri::menu::MenuItemBuilder::with_id("quit", "Quit").build(app)?;
            let menu = tauri::menu::MenuBuilder::new(app)
                .items(&[&show_item, &quit_item])
                .build()?;

            // System tray icon
            let _tray = TrayIconBuilder::with_id("main")
                .icon(create_app_icon())
                .tooltip("SpotPeek")
                .menu(&menu)
                .on_menu_event(|app, event| {
                    match event.id().as_ref() {
                        "show" => {
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.set_focus();
                                let _ = window.move_window(Position::TrayCenter);
                            }
                        }
                        "quit" => {
                            app.exit(0);
                        }
                        _ => {}
                    }
                })
                .on_tray_icon_event(move |tray, event| {
                    tauri_plugin_positioner::on_tray_event(tray.app_handle(), &event);
                    match event {
                        TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        } => {
                            let app = tray.app_handle();
                            if let Some(window) = app.get_webview_window("main") {
                                if window.is_visible().unwrap_or(false) {
                                    let _ = window.hide();
                                } else {
                                    let _ = window.show();
                                    let _ = window.set_focus();
                                    let _ = window.move_window(Position::TrayCenter);
                                }
                            }
                        }
                        _ => {}
                    }
                })
                .build(app)?;

            // Global shortcut: Ctrl+Alt+S to toggle
            #[cfg(desktop)]
            {
                use tauri_plugin_global_shortcut::{
                    Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState,
                };
                let toggle_shortcut = Shortcut::new(
                    Some(Modifiers::CONTROL | Modifiers::ALT),
                    Code::KeyS,
                );
                app.handle().plugin(
                    tauri_plugin_global_shortcut::Builder::new()
                        .with_handler(move |app, shortcut, event| {
                            if shortcut == &toggle_shortcut {
                                match event.state() {
                                    ShortcutState::Pressed => {
                                        if let Some(window) = app.get_webview_window("main") {
                                            if window.is_visible().unwrap_or(false) {
                                                let _ = window.hide();
                                            } else {
                                                let _ = window.show();
                                                let _ = window.set_focus();
                                            }
                                        }
                                    }
                                    ShortcutState::Released => {}
                                }
                            }
                        })
                        .build(),
                )?;
                app.global_shortcut().register(toggle_shortcut)?;
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            start_spotify_login,
            get_spotify_tokens,
            logout_spotify,
            get_client_id,
            set_client_id,
            get_current_playback,
            control_playback,
            seek_playback,
            set_volume,
            show_window,
            hide_window,
            toggle_window,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

