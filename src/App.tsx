import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  Play,
  Pause,
  SkipBack,
  SkipForward,
  LogIn,
  LogOut,
  X,
  Music,
} from "lucide-react";
import "./styles.css";

interface Track {
  id: string;
  name: string;
  artists: { name: string }[];
  album: {
    name: string;
    images: { url: string; height?: number; width?: number }[];
  };
  duration_ms: number;
}

interface PlaybackState {
  is_playing: boolean;
  progress_ms: number;
  item?: Track;
  shuffle_state: boolean;
  repeat_state: string;
  volume_percent?: number;
}

function App() {
  const [isLoggedIn, setIsLoggedIn] = useState<boolean | null>(null);
  const [playback, setPlayback] = useState<PlaybackState | null>(null);
  const [clientId, setClientId] = useState<string>("");
  const [hasClientId, setHasClientId] = useState<boolean | null>(null);
  const [error, setError] = useState<string>("");
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const [isSeekDragging, setIsSeekDragging] = useState(false);

  const checkAuth = async () => {
    try {
      const tokens = await invoke<{
        access_token: string;
        refresh_token?: string;
        expires_at?: number;
      } | null>("get_spotify_tokens");
      setIsLoggedIn(!!tokens);
    } catch (e) {
      setIsLoggedIn(false);
    }
  };

  const checkClientId = async () => {
    try {
      const id = await invoke<string | null>("get_client_id");
      if (id) {
        setClientId(id);
        setHasClientId(true);
      } else {
        setHasClientId(false);
      }
    } catch (e) {
      setHasClientId(false);
    }
  };

  const saveClientId = async () => {
    if (!clientId.trim()) return;
    try {
      await invoke("set_client_id", { clientId: clientId.trim() });
      setHasClientId(true);
      setError("");
    } catch (e) {
      if (typeof e === "string") setError(e);
    }
  };

  const fetchPlayback = async () => {
    try {
      const state = await invoke<PlaybackState | null>("get_current_playback");
      setPlayback(state);
      setError("");
    } catch (e) {
      if (typeof e === "string") {
        setError(e);
      }
    }
  };

  const startPolling = () => {
    if (intervalRef.current) clearInterval(intervalRef.current);
    intervalRef.current = setInterval(fetchPlayback, 2000);
  };

  const stopPolling = () => {
    if (intervalRef.current) {
      clearInterval(intervalRef.current);
      intervalRef.current = null;
    }
  };

  useEffect(() => {
    checkAuth();
    checkClientId();
    return () => stopPolling();
  }, []);

  useEffect(() => {
    if (isLoggedIn) {
      fetchPlayback();
      startPolling();
    } else {
      stopPolling();
      setPlayback(null);
    }
    return () => stopPolling();
  }, [isLoggedIn]);

  const handleLogin = async () => {
    try {
      const url = await invoke<string>("start_spotify_login");
      await openUrl(url);
      let attempts = 0;
      const poll = setInterval(async () => {
        attempts++;
        await checkAuth();
        if (attempts > 30) clearInterval(poll);
      }, 2000);
    } catch (e) {
      if (typeof e === "string") setError(e);
    }
  };

  const handleLogout = async () => {
    await invoke("logout_spotify");
    setIsLoggedIn(false);
  };

  const handleControl = async (action: string) => {
    try {
      await invoke("control_playback", { action });
      setTimeout(fetchPlayback, 400);
    } catch (e) {
      if (typeof e === "string") setError(e);
    }
  };

  const startSeekDrag = (e: React.MouseEvent<HTMLDivElement>) => {
    if (!playback?.item) return;
    e.preventDefault();
    const track = e.currentTarget;
    const fill = track.querySelector(".progress-fill") as HTMLElement | null;
    if (!fill) return;
    const duration = playback.item.duration_ms;
    setIsSeekDragging(true);

    const getRatio = (clientX: number) => {
      const rect = track.getBoundingClientRect();
      return Math.max(0, Math.min(1, (clientX - rect.left) / rect.width));
    };

    const update = (clientX: number) => {
      if (!document.body.contains(track)) return;
      fill.style.width = `${getRatio(clientX) * 100}%`;
    };

    update(e.clientX);

    const onMove = (ev: MouseEvent) => update(ev.clientX);
    const onUp = async (ev: MouseEvent) => {
      document.removeEventListener("mousemove", onMove);
      document.removeEventListener("mouseup", onUp);
      setIsSeekDragging(false);
      const ratio = getRatio(ev.clientX);
      const positionMs = Math.floor(ratio * duration);
      try {
        await invoke("seek_playback", { positionMs });
        setTimeout(fetchPlayback, 400);
      } catch (err) {
        if (typeof err === "string") setError(err);
      }
    };

    document.addEventListener("mousemove", onMove);
    document.addEventListener("mouseup", onUp);
  };

  const handleHide = async () => {
    await invoke("hide_window");
  };

  const progressPercent = isSeekDragging
    ? undefined
    : playback?.item
    ? Math.min(100, (playback.progress_ms / playback.item.duration_ms) * 100)
    : 0;


  const albumArt = playback?.item?.album?.images?.[0]?.url || "";

  const fmtTime = (ms: number) => {
    const s = Math.floor(ms / 1000);
    const m = Math.floor(s / 60);
    const sec = s % 60;
    return `${m}:${sec.toString().padStart(2, "0")}`;
  };

  const curTime = playback?.progress_ms ?? 0;
  const totalTime = playback?.item?.duration_ms ?? 0;

  if (isLoggedIn === null) {
    return (
      <div className="card" data-tauri-drag-region>
        <div className="loading">
          <span className="spinner" />
          <span className="loading-text">SpotPeek</span>
        </div>
      </div>
    );
  }

  if (!isLoggedIn) {
    return (
      <div className="card" data-tauri-drag-region>
        <button className="top-close" onClick={handleHide} aria-label="Hide">
          <X size={14} />
        </button>
        <div className="login-view">
          <div className="brand">
            <div className="brand-icon">
              <Music size={24} />
            </div>
            <div className="brand-text">
              <span className="brand-title">SpotPeek</span>
              <span className="brand-sub">Mini player for Spotify</span>
            </div>
          </div>

          {error && <p className="error-text">{error}</p>}

          {hasClientId === false ? (
            <div className="client-id-form">
              <div className="input-wrap">
                <input
                  type="text"
                  className="client-id-input"
                  placeholder="Spotify Client ID"
                  value={clientId}
                  onChange={(e) => setClientId(e.currentTarget.value)}
                  onKeyDown={(e) => e.key === "Enter" && saveClientId()}
                />
              </div>
              <button className="btn-primary" onClick={saveClientId}>
                Save Client ID
              </button>
              <p className="hint">
                Find this at developer.spotify.com/dashboard
              </p>
            </div>
          ) : (
            <button className="btn-primary" onClick={handleLogin}>
              <LogIn size={16} />
              Connect Spotify
            </button>
          )}
        </div>
      </div>
    );
  }

  const artistNames = playback?.item?.artists.map((a) => a.name).join(", ") || "";

  return (
    <div className="card" data-tauri-drag-region>
      <div className="player">
        <div className="artwork">
          {albumArt ? (
            <img src={albumArt} alt="" draggable={false} />
          ) : (
            <div className="artwork-placeholder">
              <Music size={24} />
            </div>
          )}
          {playback?.is_playing && <span className="playing-dot" />}
        </div>

        <div className="meta">
          <div className="meta-text">
            <div className="track-name">{playback?.item?.name || "Nothing playing"}</div>
            <div className="artist-name">
              {artistNames || "Open Spotify and play something"}
            </div>
          </div>

          <div className="progress-wrap">
            <span className="progress-time">{fmtTime(curTime)}</span>
            <div className="progress-track" onMouseDown={startSeekDrag}>
              <div
                className="progress-fill"
                style={{ width: progressPercent !== undefined ? `${progressPercent}%` : undefined }}
              />
            </div>
            <span className="progress-time">{fmtTime(totalTime)}</span>
          </div>

          <div className="controls">
            <button
              className="ctrl-btn"
              onClick={() => handleControl("previous")}
              aria-label="Previous"
            >
              <SkipBack size={16} />
            </button>
            <button
              className="ctrl-btn ctrl-main"
              onClick={() =>
                handleControl(playback?.is_playing ? "pause" : "play")
              }
              aria-label={playback?.is_playing ? "Pause" : "Play"}
            >
              {playback?.is_playing ? (
                <Pause size={18} />
              ) : (
                <Play size={18} />
              )}
            </button>
            <button
              className="ctrl-btn"
              onClick={() => handleControl("next")}
              aria-label="Next"
            >
              <SkipForward size={16} />
            </button>
          </div>
          {error && <p className="error-text">{error}</p>}
        </div>
      </div>

      <div className="top-actions">
        <button className="icon-btn" onClick={handleLogout} aria-label="Logout">
          <LogOut size={12} />
        </button>
        <button className="icon-btn" onClick={handleHide} aria-label="Hide">
          <X size={12} />
        </button>
      </div>
    </div>
  );
}

export default App;
