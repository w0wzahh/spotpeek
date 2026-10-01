use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

const SPOTIFY_API: &str = "https://api.spotify.com/v1";

// One client for the whole process — connection reuse + a hard timeout so a
// stalled request can never wedge an invoke (and freeze the UI) forever.
pub(crate) static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .connect_timeout(std::time::Duration::from_secs(5))
        .build()
        .expect("failed to build HTTP client")
});

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub id: String,
    pub name: String,
    pub artists: Vec<Artist>,
    pub album: Album,
    pub duration_ms: u64,
    pub external_urls: Option<ExternalUrls>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalUrls {
    pub spotify: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artist {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Album {
    pub name: String,
    pub images: Vec<Image>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Image {
    pub url: String,
    pub height: Option<u32>,
    pub width: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub id: Option<String>,
    pub name: String,
    #[serde(rename = "type")]
    pub device_type: Option<String>,
    #[serde(default)]
    pub is_active: bool,
    pub volume_percent: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaybackState {
    pub is_playing: bool,
    pub progress_ms: u64,
    pub item: Option<Track>,
    pub shuffle_state: bool,
    pub repeat_state: String,
    pub volume_percent: Option<u8>,
    pub device: Option<Device>,
}

pub async fn get_currently_playing(access_token: &str) -> Result<Option<PlaybackState>, String> {
    let res = CLIENT
        .get(format!("{}/me/player", SPOTIFY_API))
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if res.status() == 204 {
        return Ok(None);
    }

    if !res.status().is_success() {
        return Err(format!("Spotify API error: {}", res.status()));
    }

    let body = res.json::<serde_json::Value>().await.map_err(|e| e.to_string())?;

    let item = body.get("item").and_then(|i| {
        if i.is_null() {
            None
        } else {
            serde_json::from_value(i.clone()).ok()
        }
    });

    let volume_percent = body
        .get("device")
        .and_then(|d| d.get("volume_percent"))
        .and_then(|v| v.as_u64())
        .map(|v| v as u8);

    let device = body.get("device").and_then(|d| {
        if d.is_null() {
            None
        } else {
            serde_json::from_value(d.clone()).ok()
        }
    });

    Ok(Some(PlaybackState {
        is_playing: body["is_playing"].as_bool().unwrap_or(false),
        progress_ms: body["progress_ms"].as_u64().unwrap_or(0),
        item,
        shuffle_state: body["shuffle_state"].as_bool().unwrap_or(false),
        repeat_state: body["repeat_state"].as_str().unwrap_or("off").to_string(),
        volume_percent,
        device,
    }))
}

async fn put_empty(url: String, access_token: &str) -> Result<(), String> {
    let res = CLIENT
        .put(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .header("Content-Type", "application/json")
        .body("{}")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if res.status().is_success() || res.status() == 204 {
        Ok(())
    } else {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        Err(format!("Spotify error: {} — {}", status, body))
    }
}

pub async fn seek_playback(access_token: &str, position_ms: u64) -> Result<(), String> {
    put_empty(
        format!("{}/me/player/seek?position_ms={}", SPOTIFY_API, position_ms),
        access_token,
    )
    .await
}

pub async fn set_volume(access_token: &str, volume_percent: u8) -> Result<(), String> {
    put_empty(
        format!("{}/me/player/volume?volume_percent={}", SPOTIFY_API, volume_percent),
        access_token,
    )
    .await
}

pub async fn set_shuffle(access_token: &str, state: bool) -> Result<(), String> {
    put_empty(
        format!("{}/me/player/shuffle?state={}", SPOTIFY_API, state),
        access_token,
    )
    .await
}

pub async fn set_repeat(access_token: &str, state: &str) -> Result<(), String> {
    if !matches!(state, "off" | "context" | "track") {
        return Err("Invalid repeat state".to_string());
    }
    put_empty(
        format!("{}/me/player/repeat?state={}", SPOTIFY_API, state),
        access_token,
    )
    .await
}

pub async fn set_track_saved(access_token: &str, track_id: &str, saved: bool) -> Result<(), String> {
    // Spotify 411s on PUT/DELETE without a Content-Length — send the ids as a JSON body.
    let url = format!("{}/me/tracks", SPOTIFY_API);
    let body = serde_json::json!({ "ids": [track_id] });
    let req = if saved {
        CLIENT.put(&url)
    } else {
        CLIENT.delete(&url)
    };
    let res = req
        .header("Authorization", format!("Bearer {}", access_token))
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if res.status().is_success() || res.status() == 204 {
        Ok(())
    } else {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        Err(format!("Save track error: {} — {}", status, body))
    }
}

pub async fn get_devices(access_token: &str) -> Result<Vec<Device>, String> {
    let res = CLIENT
        .get(format!("{}/me/player/devices", SPOTIFY_API))
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("Spotify API error: {}", res.status()));
    }

    let body = res.json::<serde_json::Value>().await.map_err(|e| e.to_string())?;
    let devices = body
        .get("devices")
        .and_then(|d| serde_json::from_value::<Vec<Device>>(d.clone()).ok())
        .unwrap_or_default();
    Ok(devices)
}

pub async fn transfer_playback(access_token: &str, device_id: &str) -> Result<(), String> {
    let res = CLIENT
        .put(format!("{}/me/player", SPOTIFY_API))
        .header("Authorization", format!("Bearer {}", access_token))
        .json(&serde_json::json!({ "device_ids": [device_id], "play": true }))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if res.status().is_success() || res.status() == 204 {
        Ok(())
    } else {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        Err(format!("Transfer error: {} — {}", status, body))
    }
}

pub async fn is_track_saved(access_token: &str, track_id: &str) -> Result<bool, String> {
    let res = CLIENT
        .get(format!("{}/me/tracks/contains?ids={}", SPOTIFY_API, track_id))
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("Spotify API error: {}", res.status()));
    }

    let body = res.json::<serde_json::Value>().await.map_err(|e| e.to_string())?;
    Ok(body.get(0).and_then(|v| v.as_bool()).unwrap_or(false))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Playlist {
    pub id: String,
    pub name: String,
    pub image: Option<String>,
    pub track_count: u32,
}

pub async fn get_playlists(access_token: &str) -> Result<Vec<Playlist>, String> {
    let res = CLIENT
        .get(format!("{}/me/playlists?limit=50", SPOTIFY_API))
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        return Err(format!("Playlists error: {} — {}", status, body));
    }

    let body = res.json::<serde_json::Value>().await.map_err(|e| e.to_string())?;
    let playlists = body
        .get("items")
        .and_then(|i| i.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|p| {
                    Some(Playlist {
                        id: p.get("id")?.as_str()?.to_string(),
                        name: p.get("name")?.as_str()?.to_string(),
                        image: p
                            .get("images")
                            .and_then(|i| i.as_array())
                            .and_then(|i| i.first())
                            .and_then(|i| i.get("url"))
                            .and_then(|u| u.as_str())
                            .map(String::from),
                        track_count: p
                            .get("tracks")
                            .and_then(|t| t.get("total"))
                            .and_then(|t| t.as_u64())
                            .unwrap_or(0) as u32,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(playlists)
}

pub async fn save_to_playlist(
    access_token: &str,
    playlist_id: &str,
    track_id: &str,
) -> Result<(), String> {
    let res = CLIENT
        .post(format!("{}/playlists/{}/items", SPOTIFY_API, playlist_id))
        .header("Authorization", format!("Bearer {}", access_token))
        .json(&serde_json::json!({
            "uris": [format!("spotify:track:{}", track_id)]
        }))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if res.status().is_success() {
        Ok(())
    } else {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        Err(format!("Add to playlist error: {} — {}", status, body))
    }
}

/// First real track in the user's queue (skips episodes/podcast entries).
pub async fn get_up_next(access_token: &str) -> Result<Option<Track>, String> {
    let res = CLIENT
        .get(format!("{}/me/player/queue", SPOTIFY_API))
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if res.status() == 204 {
        return Ok(None);
    }
    if !res.status().is_success() {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        return Err(format!("Queue error: {} — {}", status, body));
    }

    let body = res.json::<serde_json::Value>().await.map_err(|e| e.to_string())?;
    let next = body
        .get("queue")
        .and_then(|q| q.as_array())
        .and_then(|q| {
            q.iter()
                .find(|i| i.get("type").and_then(|t| t.as_str()) == Some("track"))
        })
        .and_then(|t| serde_json::from_value(t.clone()).ok());
    Ok(next)
}

pub async fn control_playback(access_token: &str, action: &str) -> Result<(), String> {
    let method = match action {
        "play" | "pause" => reqwest::Method::PUT,
        "next" | "previous" => reqwest::Method::POST,
        _ => return Err("Invalid action".to_string()),
    };
    let url = format!("{}/me/player/{}", SPOTIFY_API, action);

    let res = CLIENT
        .request(method, &url)
        .header("Authorization", format!("Bearer {}", access_token))
        .header("Content-Type", "application/json")
        .body("{}")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if res.status().is_success() || res.status() == 204 {
        Ok(())
    } else {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        Err(format!("Control error: {} — {}", status, body))
    }
}
