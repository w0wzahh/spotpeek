mod auth;
mod media;
mod spotify;
mod voice;

use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::{
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, PhysicalPosition, WebviewWindow, Wry,
};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use tauri_plugin_store::StoreExt;

/// Currently registered toggle shortcut (rebindable from settings).
static CURRENT_HOTKEY: Mutex<Option<Shortcut>> = Mutex::new(None);
const DEFAULT_HOTKEY: &str = "Control+Alt+KeyS";

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
    store.set(
        "spotify_tokens",
        serde_json::to_value(tokens).map_err(|e| e.to_string())?,
    );
    store.save().map_err(|e| e.to_string())?;
    Ok(())
}

fn clear_tokens(app: &tauri::AppHandle<Wry>) -> Result<(), String> {
    let store = app.store("store.json").map_err(|e| e.to_string())?;
    store.delete("spotify_tokens");
    store.save().map_err(|e| e.to_string())?;
    Ok(())
}

/// Runs `f` with a valid access token, transparently refreshing on a 401.
pub(crate) async fn with_access_token<T, F, Fut>(app: &tauri::AppHandle<Wry>, f: F) -> Result<T, String>
where
    F: Fn(String) -> Fut,
    Fut: std::future::Future<Output = Result<T, String>>,
{
    let tokens = load_tokens(app)?.ok_or_else(|| "Not authenticated".to_string())?;

    match f(tokens.access_token.clone()).await {
        Err(e) if e.contains("401") => {
            let refresh = tokens
                .refresh_token
                .clone()
                .ok_or_else(|| "Session expired. Please log in again.".to_string())?;
            let store = app.store("store.json").map_err(|e| e.to_string())?;
            let client_id = store
                .get("client_id")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .ok_or_else(|| "Client ID missing".to_string())?;
            let new_token = match auth::refresh_access_token(&refresh, &client_id).await {
                Ok(t) => t,
                Err(e) => {
                    // Only nuke the session when Spotify actually rejected the
                    // refresh token (400 invalid_grant) — a transient network
                    // failure shouldn't log the user out.
                    if e.contains("400") {
                        let _ = clear_tokens(app);
                        let _ = app.emit("island-logout", ());
                    }
                    return Err(e);
                }
            };
            let new_tokens = SpotifyTokens {
                access_token: new_token.access_token.clone(),
                refresh_token: new_token.refresh_token.or_else(|| Some(refresh)),
                expires_at: Some(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs()
                        + new_token.expires_in,
                ),
            };
            save_tokens(app, &new_tokens)?;
            f(new_tokens.access_token).await
        }
        other => other,
    }
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
            "user-library-read",
            "user-library-modify",
            "playlist-read-private",
            "playlist-read-collaborative",
            "playlist-modify-public",
            "playlist-modify-private",
        ],
    );

    let app_handle = app.clone();
    tokio::spawn(async move {
        let result = auth::start_callback_server(
            flow.csrf_token.clone(),
            client_id.clone(),
            flow.pkce_verifier.clone(),
        )
        .await;
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
    Ok(store
        .get("client_id")
        .and_then(|v| v.as_str().map(|s| s.to_string())))
}

#[tauri::command]
async fn set_client_id(app: tauri::AppHandle<Wry>, client_id: String) -> Result<(), String> {
    let store = app.store("store.json").map_err(|e| e.to_string())?;
    store.set("client_id", serde_json::json!(client_id));
    store.save().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn get_current_playback(
    app: tauri::AppHandle<Wry>,
) -> Result<Option<spotify::PlaybackState>, String> {
    if load_tokens(&app)?.is_none() {
        return Ok(None);
    }
    with_access_token(&app, |t| async move { spotify::get_currently_playing(&t).await }).await
}

#[tauri::command]
async fn control_playback(app: tauri::AppHandle<Wry>, action: String) -> Result<(), String> {
    with_access_token(&app, |t| {
        let action = action.clone();
        async move { spotify::control_playback(&t, &action).await }
    })
    .await
}

#[tauri::command]
async fn seek_playback(app: tauri::AppHandle<Wry>, position_ms: u64) -> Result<(), String> {
    with_access_token(&app, |t| async move { spotify::seek_playback(&t, position_ms).await }).await
}

#[tauri::command]
async fn set_volume(app: tauri::AppHandle<Wry>, volume_percent: u8) -> Result<(), String> {
    with_access_token(&app, |t| async move { spotify::set_volume(&t, volume_percent).await }).await
}

#[tauri::command]
async fn set_shuffle(app: tauri::AppHandle<Wry>, state: bool) -> Result<(), String> {
    with_access_token(&app, |t| async move { spotify::set_shuffle(&t, state).await }).await
}

#[tauri::command]
async fn set_repeat(app: tauri::AppHandle<Wry>, state: String) -> Result<(), String> {
    with_access_token(&app, |t| {
        let state = state.clone();
        async move { spotify::set_repeat(&t, &state).await }
    })
    .await
}

#[tauri::command]
async fn set_track_saved(
    app: tauri::AppHandle<Wry>,
    track_id: String,
    saved: bool,
) -> Result<(), String> {
    with_access_token(&app, |t| {
        let track_id = track_id.clone();
        async move { spotify::set_track_saved(&t, &track_id, saved).await }
    })
    .await
}

#[tauri::command]
async fn is_track_saved(app: tauri::AppHandle<Wry>, track_id: String) -> Result<bool, String> {
    with_access_token(&app, |t| {
        let track_id = track_id.clone();
        async move { spotify::is_track_saved(&t, &track_id).await }
    })
    .await
}

#[tauri::command]
async fn get_devices(app: tauri::AppHandle<Wry>) -> Result<Vec<spotify::Device>, String> {
    with_access_token(&app, |t| async move { spotify::get_devices(&t).await }).await
}

#[tauri::command]
async fn transfer_playback(app: tauri::AppHandle<Wry>, device_id: String) -> Result<(), String> {
    with_access_token(&app, |t| {
        let device_id = device_id.clone();
        async move { spotify::transfer_playback(&t, &device_id).await }
    })
    .await
}

#[tauri::command]
async fn get_up_next(app: tauri::AppHandle<Wry>) -> Result<Option<spotify::Track>, String> {
    if load_tokens(&app)?.is_none() {
        return Ok(None);
    }
    with_access_token(&app, |t| async move { spotify::get_up_next(&t).await }).await
}

#[tauri::command]
async fn get_playlists(app: tauri::AppHandle<Wry>) -> Result<Vec<spotify::Playlist>, String> {
    with_access_token(&app, |t| async move { spotify::get_playlists(&t).await }).await
}

#[tauri::command]
async fn save_to_playlist(
    app: tauri::AppHandle<Wry>,
    playlist_id: String,
    track_id: String,
) -> Result<(), String> {
    with_access_token(&app, |t| {
        let playlist_id = playlist_id.clone();
        let track_id = track_id.clone();
        async move { spotify::save_to_playlist(&t, &playlist_id, &track_id).await }
    })
    .await
}

#[tauri::command]
async fn snap_island(app: tauri::AppHandle<Wry>, corner: String) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "Window missing".to_string())?;
    if let Ok(Some(monitor)) = window.primary_monitor() {
        let screen = monitor.size();
        let origin = monitor.position();
        let win = window
            .outer_size()
            .unwrap_or(tauri::PhysicalSize::new(400, 104));
        const M: i32 = 10;
        let x = match corner.as_str() {
            "top-left" => origin.x + M,
            "top-right" => origin.x + (screen.width as i32 - win.width as i32) - M,
            _ => origin.x + (screen.width as i32 - win.width as i32) / 2,
        };
        let y = origin.y + M;
        let _ = window.set_position(PhysicalPosition::new(x, y));
        let store = read_store(&app)?;
        store.set("window_pos", serde_json::json!([x, y]));
        store.save().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
async fn save_window_position(app: tauri::AppHandle<Wry>) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        let pos = window.outer_position().map_err(|e| e.to_string())?;
        let store = app.store("store.json").map_err(|e| e.to_string())?;
        store.set("window_pos", serde_json::json!([pos.x, pos.y]));
        store.save().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Snap the window to the top-center of the primary monitor, Dynamic Island style.
fn position_top_center(window: &WebviewWindow<Wry>) {
    if let Ok(Some(monitor)) = window.primary_monitor() {
        let screen = monitor.size();
        let origin = monitor.position();
        let win = window.outer_size().unwrap_or(tauri::PhysicalSize::new(400, 104));
        let x = origin.x + (screen.width as i32 - win.width as i32) / 2;
        let y = origin.y + 10;
        let _ = window.set_position(PhysicalPosition::new(x, y));
    }
}

/// Restore the last dragged position, falling back to top-center.
fn restore_position(app: &tauri::AppHandle<Wry>, window: &WebviewWindow<Wry>) {
    let saved = app
        .store("store.json")
        .ok()
        .and_then(|s| s.get("window_pos"))
        .and_then(|v| {
            let x = v.get(0).and_then(|n| n.as_i64())?;
            let y = v.get(1).and_then(|n| n.as_i64())?;
            Some((x as i32, y as i32))
        });
    match saved {
        Some((x, y)) => {
            // Clamp into the primary monitor — a saved position can be
            // off-screen if a display was unplugged or resolution changed.
            if let Ok(Some(monitor)) = window.primary_monitor() {
                let screen = monitor.size();
                let origin = monitor.position();
                let win = window
                    .outer_size()
                    .unwrap_or(tauri::PhysicalSize::new(400, 104));
                let cx = x.clamp(
                    origin.x,
                    origin.x + (screen.width as i32 - win.width as i32).max(0),
                );
                let cy = y.clamp(
                    origin.y,
                    origin.y + (screen.height as i32 - win.height as i32).max(0),
                );
                let _ = window.set_position(PhysicalPosition::new(cx, cy));
            } else {
                let _ = window.set_position(PhysicalPosition::new(x, y));
            }
        }
        None => position_top_center(window),
    }
}

/// Put WebView2 into low-memory mode while the island is hidden — the widget
/// spends most of its life hidden, so let the renderer trim its working set.
#[cfg(target_os = "windows")]
fn set_webview_memory_level(window: &WebviewWindow<Wry>, low: bool) {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2_19, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW,
        COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL,
    };
    use windows_core::Interface;
    let target = if low {
        COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW
    } else {
        COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL
    };
    let _ = window.with_webview(move |webview| unsafe {
        let _ = webview
            .controller()
            .CoreWebView2()
            .and_then(|wv| wv.cast::<ICoreWebView2_19>())
            .and_then(|wv| wv.SetMemoryUsageTargetLevel(target));
    });
}

#[cfg(not(target_os = "windows"))]
fn set_webview_memory_level(_window: &WebviewWindow<Wry>, _low: bool) {}

pub(crate) fn show_island(app: &tauri::AppHandle<Wry>) {
    if let Some(window) = app.get_webview_window("main") {
        set_webview_memory_level(&window, false);
        restore_position(app, &window);
        let _ = window.show();
        let _ = window.set_focus();
        let _ = app.emit("island-visibility", true);
    }
}

pub(crate) fn hide_island(app: &tauri::AppHandle<Wry>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
        let _ = app.emit("island-visibility", false);
        set_webview_memory_level(&window, true);
    }
}

/// Resize the island window. Runs on the async runtime — synchronous
/// set_size calls on the main thread can freeze the window on Windows
/// (tauri-apps/tauri#3990).
#[tauri::command]
async fn set_island_size(
    app: tauri::AppHandle<Wry>,
    expanded: bool,
    ui_scale: Option<f64>,
) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "Window missing".to_string())?;
    // ui_scale multiplies the CSS-zoomed island — without it the enlarged
    // island would run off the bottom of the window and clip flat.
    let ui = ui_scale.unwrap_or(1.0).clamp(0.7, 1.6);
    let (w, h) = if expanded { (400.0, 264.0) } else { (400.0, 104.0) };
    let (w, h) = ((w * ui).ceil(), (h * ui).ceil());
    let scale = window.scale_factor().unwrap_or(1.0);
    let target = tauri::LogicalSize::new(w, h).to_physical::<u32>(scale);
    if window.inner_size().map_err(|e| e.to_string())? == target {
        return Ok(());
    }
    window
        .set_size(tauri::Size::Logical(tauri::LogicalSize::new(w, h)))
        .map_err(|e| e.to_string())
}

/* ---------- settings ---------- */

fn read_store(app: &tauri::AppHandle<Wry>) -> Result<std::sync::Arc<tauri_plugin_store::Store<Wry>>, String> {
    app.store("store.json").map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_settings(app: tauri::AppHandle<Wry>) -> Result<serde_json::Value, String> {
    let store = read_store(&app)?;
    let get_bool = |k: &str, d: bool| {
        store.get(k).and_then(|v| v.as_bool()).unwrap_or(d)
    };
    Ok(serde_json::json!({
        "hotkey": store
            .get("hotkey")
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_else(|| DEFAULT_HOTKEY.to_string()),
        "always_on_top": get_bool("always_on_top", true),
        "pop_on_change": get_bool("pop_on_change", true),
        "auto_hide_idle": get_bool("auto_hide_idle", true),
        "voice_enabled": get_bool("voice_enabled", false),
        "launch_at_login": app.autolaunch().is_enabled().unwrap_or(false),
        "accent": store
            .get("accent")
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_else(|| "#1db954".to_string()),
        "ui_scale": store
            .get("ui_scale")
            .and_then(|v| v.as_f64())
            .unwrap_or(1.0),
        "island_opacity": store
            .get("island_opacity")
            .and_then(|v| v.as_f64())
            .unwrap_or(1.0),
    }))
}

#[tauri::command]
async fn update_setting(
    app: tauri::AppHandle<Wry>,
    key: String,
    value: serde_json::Value,
) -> Result<(), String> {
    let store = read_store(&app)?;
    store.set(&key, value.clone());
    store.save().map_err(|e| e.to_string())?;

    match key.as_str() {
        "always_on_top" => {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.set_always_on_top(value.as_bool().unwrap_or(true));
            }
        }
        "launch_at_login" => {
            let _ = if value.as_bool().unwrap_or(false) {
                app.autolaunch().enable()
            } else {
                app.autolaunch().disable()
            };
        }
        "voice_enabled" => {
            voice::set_enabled(&app, value.as_bool().unwrap_or(false));
        }
        _ => {}
    }
    Ok(())
}

#[tauri::command]
async fn set_hotkey(app: tauri::AppHandle<Wry>, accelerator: String) -> Result<(), String> {
    let shortcut: Shortcut = accelerator
        .parse()
        .map_err(|_| format!("Couldn't parse shortcut \"{}\"", accelerator))?;
    #[cfg(desktop)]
    {
        if let Some(old) = CURRENT_HOTKEY.lock().unwrap().replace(shortcut) {
            let _ = app.global_shortcut().unregister(old);
        }
        app.global_shortcut()
            .register(shortcut)
            .map_err(|e| format!("Shortcut unavailable: {}", e))?;
    }
    let store = read_store(&app)?;
    store.set("hotkey", serde_json::json!(accelerator));
    store.save().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn reset_island_position(app: tauri::AppHandle<Wry>) -> Result<(), String> {
    let store = read_store(&app)?;
    store.delete("window_pos");
    store.save().map_err(|e| e.to_string())?;
    if let Some(w) = app.get_webview_window("main") {
        position_top_center(&w);
    }
    Ok(())
}

#[tauri::command]
async fn show_window(app: tauri::AppHandle<Wry>) -> Result<(), String> {
    show_island(&app);
    Ok(())
}

#[tauri::command]
async fn hide_window(app: tauri::AppHandle<Wry>) -> Result<(), String> {
    hide_island(&app);
    Ok(())
}

#[tauri::command]
async fn toggle_window(app: tauri::AppHandle<Wry>) -> Result<bool, String> {
    if let Some(window) = app.get_webview_window("main") {
        if window.is_visible().unwrap_or(false) {
            hide_island(&app);
            Ok(false)
        } else {
            show_island(&app);
            Ok(true)
        }
    } else {
        Ok(false)
    }
}

// Pre-decoded 64x64 RGBA (see tools/make-icons.mjs) — keeps the binary free
// of a PNG decoder just for the tray icon.
const TRAY_ICON_RGBA: &[u8] = include_bytes!("../icons/tray.rgba");

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // First plugin: a second launch just shows the existing island
        // instead of spawning a duplicate process.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_island(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .setup(|app| {
            // Tray menu
            let show_item = tauri::menu::MenuItemBuilder::with_id("show", "Show").build(app)?;
            let autostart_item = tauri::menu::CheckMenuItemBuilder::with_id(
                "autostart",
                "Launch at login",
            )
            .checked(app.autolaunch().is_enabled().unwrap_or(false))
            .build(app)?;
            let quit_item = tauri::menu::MenuItemBuilder::with_id("quit", "Quit").build(app)?;
            let menu = tauri::menu::MenuBuilder::new(app)
                .items(&[&show_item, &autostart_item, &quit_item])
                .build()?;

            let autostart_toggle = autostart_item.clone();

            // System tray icon
            let _tray = TrayIconBuilder::with_id("main")
                .icon(tauri::image::Image::new(TRAY_ICON_RGBA, 64, 64))
                .tooltip("SpotPeek")
                .menu(&menu)
                .on_menu_event(move |app, event| match event.id().as_ref() {
                    "show" => show_island(app),
                    "autostart" => {
                        let enabled = app.autolaunch().is_enabled().unwrap_or(false);
                        let _ = if enabled {
                            app.autolaunch().disable()
                        } else {
                            app.autolaunch().enable()
                        };
                        let _ = autostart_toggle.set_checked(!enabled);
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(move |tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            if window.is_visible().unwrap_or(false) {
                                hide_island(app);
                            } else {
                                show_island(app);
                            }
                        }
                    }
                })
                .build(app)?;

            // Global shortcut (rebindable via settings; default Ctrl+Alt+S)
            #[cfg(desktop)]
            {
                let stored = app
                    .store("store.json")
                    .ok()
                    .and_then(|s| s.get("hotkey"))
                    .and_then(|v| v.as_str().map(String::from))
                    .unwrap_or_else(|| DEFAULT_HOTKEY.to_string());
                let shortcut = stored
                    .parse::<Shortcut>()
                    .unwrap_or_else(|_| DEFAULT_HOTKEY.parse().unwrap());
                *CURRENT_HOTKEY.lock().unwrap() = Some(shortcut);

                app.handle().plugin(
                    tauri_plugin_global_shortcut::Builder::new()
                        .with_handler(move |app, shortcut, event| {
                            let is_toggle = CURRENT_HOTKEY
                                .lock()
                                .ok()
                                .and_then(|g| *g)
                                .map(|s| &s == shortcut)
                                .unwrap_or(false);
                            if is_toggle && event.state() == ShortcutState::Pressed {
                                if let Some(window) = app.get_webview_window("main") {
                                    if window.is_visible().unwrap_or(false) {
                                        hide_island(app);
                                    } else {
                                        show_island(app);
                                    }
                                }
                            }
                        })
                        .build(),
                )?;
                // Non-fatal: if another app owns it we still work via the tray.
                if let Err(e) = app.global_shortcut().register(shortcut) {
                    eprintln!("Global shortcut unavailable: {}", e);
                }
            }

            // Voice control if previously enabled
            let voice_on = app
                .store("store.json")
                .ok()
                .and_then(|s| s.get("voice_enabled"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if voice_on {
                voice::set_enabled(&app.handle(), true);
            }

            // SMTC watcher — instant "something changed" events so track
            // pops and music-start detection don't wait on the HTTP poll.
            media::start(app.handle().clone());

            Ok(())
        })
        // Drag-end detection: startDragging() hands control to the OS modal
        // move loop, so the final mouseup never reaches the webview. Instead
        // we debounce Moved events — when they stop, the drag has ended.
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Moved(_) = event {
                static MOVE_TICK: std::sync::atomic::AtomicU64 =
                    std::sync::atomic::AtomicU64::new(0);
                let tick = MOVE_TICK.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                let app = window.app_handle().clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(220)).await;
                    if MOVE_TICK.load(std::sync::atomic::Ordering::SeqCst) == tick {
                        let _ = app.emit("island-drag-end", ());
                    }
                });
            }
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
            set_shuffle,
            set_repeat,
            set_track_saved,
            is_track_saved,
            get_devices,
            transfer_playback,
            get_up_next,
            get_playlists,
            save_to_playlist,
            snap_island,
            save_window_position,
            set_island_size,
            show_window,
            hide_window,
            toggle_window,
            get_settings,
            update_setting,
            set_hotkey,
            reset_island_position,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
