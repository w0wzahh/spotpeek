# SpotPeek

A tiny Spotify mini-player for Windows 11. It sits in your system tray and pops up a small floating card showing what you are currently playing, along with playback controls.

Built with Tauri v2 (Rust backend, React frontend).

## What it does

- Shows a floating mini-player that stays on top of other windows
- Lives in the system tray; click the icon to show or hide it
- Displays the current track, artist, album art, and a progress bar
- Lets you play, pause, skip, and go back
- Stores your Spotify tokens in an encrypted local vault
- Automatically refreshes the access token when it expires

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

### Building a release

```powershell
npm run tauri build
```

## Project layout

```
spotpeek/
├── src/                  React frontend
│   ├── App.tsx
│   ├── styles.css
│   └── main.tsx
├── src-tauri/            Rust backend
│   ├── src/
│   │   ├── lib.rs        Tray icon, window, commands
│   │   ├── auth.rs       OAuth PKCE flow
│   │   ├── spotify.rs    API client
│   │   └── store.rs      Encrypted token storage
│   ├── Cargo.toml
│   └── tauri.conf.json
└── package.json
```

## Keyboard shortcut

`Ctrl + Alt + S` toggles the window.

## Notes

- The app starts hidden. Use the tray icon or the shortcut to bring it up.
- Tokens are stored in a Tauri Stronghold vault, not plain text.
- The OAuth callback runs a local server on port 9133.

## License

MIT