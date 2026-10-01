# SpotPeek

A tiny Spotify "floating island" for Windows 11 — a glass pill that docks at the top-center of your screen (Dynamic Island / Rainmeter-style) and springs open on hover with full playback controls.

Built with Tauri v2 (Rust backend, React frontend). Featherweight: no Electron, no bundled Chromium — it rides on the system WebView2 and idles at tens of MB.

## What it does

- Floating glass island, always on top, docked top-center (drag to move it)
- Collapsed pill: album art, scrolling track/artist, live EQ bars, play/pause
- Hover to expand: artwork, marquee titles, seek bar, shuffle / previous / play / next / repeat, volume slider, like-to-Liked-Songs heart, active device name
- Lives in the system tray; click the icon to show or hide it
- Progress bar interpolates smoothly between polls — no jitter
- Adaptive polling: fast while playing, slow when idle, near-zero when hidden
- Automatically refreshes the access token when it expires
- Click the artwork to open the track in Spotify

## Setup

### Prerequisites

- Node.js (LTS)
- Rust (install via [rustup](https://rustup.rs/))
- A Spotify account

### Spotify app

1. Go to the [Spotify Developer Dashboard](https://developer.spotify.com/dashboard)
2. Create an app
3. Add `http://127.0.0.1:9133/callback` as a Redirect URI
4. Copy the Client ID

### Running locally

```powershell
npm install
npm run tauri dev
```

The first time you run it, paste your Spotify Client ID into the prompt, then log in with your Spotify account.

> **Note:** if you have a `&` in your folder path (e.g. `Games_&_OTHER_Misc`), `npm` scripts can break on Windows cmd. Run `node node_modules/typescript/bin/tsc` / `node node_modules/vite/bin/vite.js` directly instead.

### Building a release

```powershell
npm run tauri build
```

## Project layout

```
spotpeek/
├── src/                  React frontend
│   ├── App.tsx           Island UI (collapsed pill / expanded card)
│   ├── styles.css        Glassmorphism + spring animations
│   └── main.tsx
├── src-tauri/            Rust backend
│   ├── src/
│   │   ├── lib.rs        Tray icon, window, commands
│   │   ├── auth.rs       OAuth PKCE flow
│   │   └── spotify.rs    Spotify Web API client
│   ├── Cargo.toml
│   └── tauri.conf.json
└── package.json
```

## Keyboard shortcut

`Ctrl + Alt + S` toggles the island.

## Notes

- The app starts hidden. Use the tray icon or the shortcut to bring it up.
- The OAuth callback runs a local server on port 9133.
- Liking songs requires the `user-library-*` scopes — re-login once if you upgraded from an older version.

## License

MIT
