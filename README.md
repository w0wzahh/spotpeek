<div align="center">

<img src="src-tauri/icons/icon.png" alt="SpotPeek" width="96" />

# SpotPeek

**A Dynamic Island for Spotify on Windows.**<br/>
A floating glass pill that lives at the top of your screen — collapsed to a
tiny pill, blooming open on hover with full playback control.

[![Release](https://img.shields.io/github/v/release/w0wzahh/spotpeek?style=flat&color=1db954)](https://github.com/w0wzahh/spotpeek/releases)
[![Platform](https://img.shields.io/badge/platform-Windows%2010%2F11-0078d4?style=flat)](https://github.com/w0wzahh/spotpeek)
[![Built with Tauri](https://img.shields.io/badge/built%20with-Tauri%20v2-24c8d8?style=flat)](https://tauri.app)
[![License](https://img.shields.io/badge/license-MIT-blue?style=flat)](LICENSE)

[Download](../../releases/latest) · [Report a bug](mailto:emrebelgrad@gmail.com) · [Ko-fi](https://ko-fi.com/w0wzahh)

</div>

---

## Why SpotPeek

Spotify's desktop app is heavy for "just show me what's playing." Rainmeter
widgets can't control playback properly. SpotPeek sits in the middle — a real
Spotify Connect client in a featherweight package:

- **~45 MB RAM** — no Electron, no bundled Chromium; it rides on the system WebView2
- **Instant reactions** — a Windows media-session (SMTC) listener detects track
  changes the moment they happen instead of waiting on API polls
- **Zero work when hidden** — WebView2 drops to low-memory mode, animations and
  progress rendering fully pause, polling throttles down

## Features

**Playback**
- Play / pause / next / previous, seek bar, shuffle, repeat
- Volume slider with mute toggle (scroll the slider to adjust)
- Like to Liked Songs · copy track link · open in Spotify
- **Spotify Connect** device picker — hop playback between devices

**Island behavior**
- Springs open on hover, collapses when you leave
- **Drag it anywhere** — it tucks compact while you carry it and blooms back on release; position persists
- Pops in when a track changes or music starts, then retracts
- Auto-hides when nothing's playing for a while (optional)
- Right-click hides instantly · `Ctrl+Alt+S` toggles (rebindable)

**Extras**
- **Synced lyrics** — karaoke-style highlighting (LRClib)
- **Save to playlist** — add the current track to any of your playlists
- **Up Next** — shows the queued track
- **Voice commands** — offline speech recognition: *peek, show, hide, play, pause, next, previous, like*
- **Settings sheet** — hotkey recorder, always-on-top, launch at login, accent colors, island size, opacity, snap-to-corner presets

## Install

Grab the latest from [**Releases**](https://github.com/w0wzahh/spotpeek/releases/latest):

- `SpotPeek_x.x.x_x64-setup.exe` — installer (recommended)
- `SpotPeek-x.x.x-portable.exe` — single file, run it directly

Requires the **WebView2 Runtime** (preinstalled on Windows 11 and most Windows 10 systems).

## First-run setup

SpotPeek talks to Spotify through your own developer app (free, ~2 minutes):

1. Open the [Spotify Developer Dashboard](https://developer.spotify.com/dashboard) → **Create app**
2. Add `http://127.0.0.1:9133/callback` as a **Redirect URI**
3. Copy the **Client ID**
4. Launch SpotPeek → paste the Client ID → log in with Spotify

A **Premium** account is required for playback controls (Spotify API limitation).

## Build from source

**Prerequisites:** [Node.js LTS](https://nodejs.org), [Rust](https://rustup.rs)

```powershell
npm install
npm run tauri dev      # dev mode (hot reload)
npm run tauri build    # release build + installer
```

> **Windows note:** if your project path contains `&` (e.g. `Games_&_Misc`),
> npm scripts and WiX bundling can break on cmd — invoke the tools directly
> (`node node_modules/vite/bin/vite.js build`) or use a path without `&`.

## Architecture

```
spotpeek/
├── src/                    React 19 frontend
│   ├── App.tsx             Island UI: collapsed pill / expanded card / sheets
│   └── styles.css          Glassmorphism, spring animations
├── src-tauri/              Rust backend (Tauri v2)
│   ├── src/
│   │   ├── lib.rs          Window, tray, hotkeys, settings, commands
│   │   ├── auth.rs         OAuth 2.0 PKCE + local callback server (:9133)
│   │   ├── spotify.rs      Spotify Web API client (timeouts, typed models)
│   │   ├── media.rs        Windows SMTC watcher → instant change events
│   │   └── voice.rs        WinRT offline speech recognition
│   └── tauri.conf.json
├── tools/make-icons.mjs    Regenerates all icon assets from icon.svg
└── dist/                   Built frontend
```

**Key design choices**

- `GET /me/player` is the source of truth; **SMTC is only the doorbell** that
  triggers an immediate refetch — keeps API usage minimal while feeling instant
- Progress bar interpolates via `requestAnimationFrame` + direct DOM writes —
  no React re-renders at 60fps, and it self-pauses when hidden
- Tokens refresh transparently; only a real `invalid_grant` logs you out —
  network blips don't kill your session
- Single-instance: a second launch just shows the existing island

## Privacy & legal

- Tokens live in `%APPDATA%\com.spotpeek.app\store.json` on your machine only
- Voice commands run **offline** via Windows' built-in speech recognition
- SpotPeek is not affiliated with or endorsed by Spotify AB. The app icon is
  Lucide's `audio-lines` (ISC), not the Spotify logo.

## Credits

Made by [w0wzahh](https://github.com/w0wzahh) — [☕ Ko-fi](https://ko-fi.com/w0wzahh)

## License

MIT — see [LICENSE](LICENSE)
