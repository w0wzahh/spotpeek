use serde::{Deserialize, Serialize};

const SPOTIFY_API: &str = "https://api.spotify.com/v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub id: String,
    pub name: String,
    pub artists: Vec<Artist>,
    pub album: Album,
    pub duration_ms: u64,
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
pub struct PlaybackState {
    pub is_playing: bool,
    pub progress_ms: u64,
    pub item: Option<Track>,
    pub shuffle_state: bool,
    pub repeat_state: String,
    pub volume_percent: Option<u8>,
}

pub async fn get_currently_playing(access_token: &str) -> Result<Option<PlaybackState>, String> {
    let client = reqwest::Client::new();
    let res = client
        .get(format!("{}/me/player/currently-playing", SPOTIFY_API))
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

    let volume_percent = body.get("device").and_then(|d| d.get("volume_percent")).and_then(|v| v.as_u64()).map(|v| v as u8);

    Ok(Some(PlaybackState {
        is_playing: body["is_playing"].as_bool().unwrap_or(false),
        progress_ms: body["progress_ms"].as_u64().unwrap_or(0),
        item,
        shuffle_state: body["shuffle_state"].as_bool().unwrap_or(false),
        repeat_state: body["repeat_state"].as_str().unwrap_or("off").to_string(),
        volume_percent,
    }))
}

pub async fn seek_playback(access_token: &str, position_ms: u64) -> Result<(), String> {
    let client = reqwest::Client::new();
    let url = format!("{}/me/player/seek?position_ms={}", SPOTIFY_API, position_ms);

    let res = client
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
        Err(format!("Seek error: {} — {}", status, body))
    }
}

pub async fn set_volume(access_token: &str, volume_percent: u8) -> Result<(), String> {
    let client = reqwest::Client::new();
    let url = format!("{}/me/player/volume?volume_percent={}", SPOTIFY_API, volume_percent);

    let res = client
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
        Err(format!("Volume error: {} — {}", status, body))
    }
}

pub async fn control_playback(access_token: &str, action: &str) -> Result<(), String> {
    let client = reqwest::Client::new();
    let method = match action {
        "play" | "pause" => reqwest::Method::PUT,
        "next" | "previous" => reqwest::Method::POST,
        _ => return Err("Invalid action".to_string()),
    };
    let url = format!("{}/me/player/{}", SPOTIFY_API, action);

    let req = client
        .request(method, &url)
        .header("Authorization", format!("Bearer {}", access_token))
        .header("Content-Type", "application/json")
        .body("{}");

    let res = req.send().await.map_err(|e| e.to_string())?;

    if res.status().is_success() || res.status() == 204 {
        Ok(())
    } else {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        Err(format!("Control error: {} — {}", status, body))
    }
}
