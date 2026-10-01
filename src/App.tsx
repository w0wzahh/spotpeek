import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  Play,
  Pause,
  SkipBack,
  SkipForward,
  Shuffle,
  Repeat,
  Repeat1,
  Heart,
  Volume2,
  Volume1,
  VolumeX,
  Speaker,
  LogIn,
  LogOut,
  X,
  Music,
  AudioLines,
  ListMusic,
  Link2,
  Check,
  Coffee,
  Settings,
  Mic,
  RotateCcw,
  ChevronLeft,
  BookOpenText,
  ListPlus,
  Github,
  Bug,
  History,
} from "lucide-react";
import "./styles.css";

const appWindow = getCurrentWindow();

interface Track {
  id: string;
  name: string;
  artists: { name: string }[];
  album: {
    name: string;
    images: { url: string; height?: number; width?: number }[];
  };
  duration_ms: number;
  external_urls?: { spotify?: string };
}

interface Playlist {
  id: string;
  name: string;
  image: string | null;
  track_count: number;
}

interface Device {
  id?: string | null;
  name: string;
  device_type?: string;
  is_active?: boolean;
}

interface PlaybackState {
  is_playing: boolean;
  progress_ms: number;
  item?: Track;
  shuffle_state: boolean;
  repeat_state: string;
  volume_percent?: number;
  device?: Device;
}

function Marquee({ text, className }: { text: string; className?: string }) {
  const ref = useRef<HTMLDivElement>(null);
  const [scrolls, setScrolls] = useState(false);
  const durRef = useRef(10);

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const overflow = el.scrollWidth > el.clientWidth + 2;
    durRef.current = Math.max(6, el.scrollWidth / 26);
    setScrolls(overflow);
  }, [text]);

  return (
    <div className={`marquee ${className ?? ""}`} ref={ref}>
      <div
        className={`marquee-inner ${scrolls ? "scrolling" : ""}`}
        style={scrolls ? { animationDuration: `${durRef.current}s` } : undefined}
      >
        <span>{text}</span>
        {scrolls && (
          <span className="marquee-dup" aria-hidden="true">
            {text}
          </span>
        )}
      </div>
    </div>
  );
}

function EqBars({ active }: { active: boolean }) {
  return (
    <div className={`eq ${active ? "on" : ""}`} aria-hidden="true">
      <span />
      <span />
      <span />
      <span />
    </div>
  );
}

const fmtTime = (ms: number) => {
  const s = Math.floor(ms / 1000);
  return `${Math.floor(s / 60)}:${(s % 60).toString().padStart(2, "0")}`;
};

// LRC parse — "[mm:ss.xx] lyric" lines → { t: ms, text }. Plain lyrics
// land at t=0 which the UI treats as "not synced" (no highlight).
const parseLrc = (raw: string) =>
  raw
    .split("\n")
    .flatMap((line) => {
      const m = /\[(\d+):(\d+(?:\.\d+)?)\](.*)/.exec(line);
      if (!m) return [];
      const t = (Number(m[1]) * 60 + Number(m[2])) * 1000;
      const text = m[3].trim();
      return text ? [{ t, text }] : [];
    })
    .sort((a, b) => a.t - b.t);

// In-app changelog — bundled with the build (the repo ships no GitHub
// releases, so history lives here). Newest first.
const CHANGELOG: { version: string; sections: [string, string[]][] }[] = [
  {
    version: "0.1.0",
    sections: [
      [
        "Added",
        [
          "Dynamic Island-style floating pill with glass blur and spring animations",
          "Full playback controls: play/pause, skip, seek, shuffle, repeat",
          "Volume slider, scroll-wheel volume on the slider, mute toggle",
          "Like/save track, copy track link, open in Spotify",
          "Spotify Connect device picker",
          "Up Next queue preview",
          "Synced lyrics (karaoke highlight) via LRClib",
          "Save current track to any playlist",
          "Track-change pop + pop when music starts (instant via Windows media sessions)",
          "Album-art color tint on the glass",
          "Settings sheet: rebindable hotkey (default Ctrl+Alt+S), always-on-top, auto-hide, launch at login",
          "Voice commands: peek, show, hide, play, pause, next, previous, like",
          "Customization: accent colors, island size, opacity, snap-to-corner",
          "Credits + GitHub + bug-report links",
        ],
      ],
      [
        "Changed",
        [
          "Rebuilt from the old rectangular mini-player into a hover-expanding island",
          "Trademark-safe Lucide audio-lines icon (no Spotify logo)",
          "Adaptive polling — faster when visible/playing, barely anything when hidden",
          "WebView2 drops to low-memory mode while hidden",
          "Progress bar runs on requestAnimationFrame — zero re-render churn",
        ],
      ],
      [
        "Fixed",
        [
          "Controls could freeze the UI — all Spotify calls now have timeouts",
          "Manual open (tray/hotkey/voice) was instantly auto-hidden when idle",
          "Drag-and-drop didn't bloom open on release (OS eats the mouseup — now detected via move events)",
          "Volume icon showed wrong state until first touch; remote volume changes now re-sync",
          "Network blips no longer log you out — only revoked sessions do",
          "Second instance crashed on a taken hotkey — single-instance + non-fatal registration",
        ],
      ],
    ],
  },
  {
    version: "0.0.x",
    sections: [
      [
        "Added",
        [
          "Original rectangular mini-player overlay",
          "Basic playback controls (play, pause, skip)",
          "Spotify OAuth sign-in flow",
          "System tray presence",
        ],
      ],
      [
        "Fixed",
        ["OAuth callback moved to 127.0.0.1 per Spotify's localhost rules"],
      ],
    ],
  },
];

// Accent presets — accent + a lighter "hi" shade, matching --accent-hi.
const ACCENTS: [string, string][] = [
  ["#1db954", "#1ed760"],
  ["#3b82f6", "#60a5fa"],
  ["#a855f7", "#c084fc"],
  ["#f97316", "#fb923c"],
  ["#ec4899", "#f472b6"],
  ["#e5e7eb", "#ffffff"],
];

function App() {
  const [isLoggedIn, setIsLoggedIn] = useState<boolean | null>(null);
  const [hasClientId, setHasClientId] = useState<boolean | null>(null);
  const [clientId, setClientId] = useState("");
  const [playback, setPlayback] = useState<PlaybackState | null>(null);
  const [isSeeking, setIsSeeking] = useState(false);
  const [saved, setSaved] = useState(false);
  const [volume, setVolume] = useState<number | null>(null);
  const [expanded, setExpanded] = useState(false);
  const [bump, setBump] = useState(false);
  const [devices, setDevices] = useState<Device[] | null>(null);
  const [showDevices, setShowDevices] = useState(false);
  // undefined = not fetched yet, null = queue empty / no next track
  const [upNext, setUpNext] = useState<Track | null | undefined>(undefined);
  const [copied, setCopied] = useState(false);
  const [peek, setPeek] = useState(false);
  const [error, setError] = useState("");
  const [showSettings, setShowSettings] = useState(false);
  const [showLyrics, setShowLyrics] = useState(false);
  const [showPlaylists, setShowPlaylists] = useState(false);
  const [showChangelog, setShowChangelog] = useState(false);
  const [clFilter, setClFilter] = useState<string | "all">("all");
  const [lyrics, setLyrics] = useState<{ t: number; text: string }[] | null | "loading">(null);
  const [lyricIdx, setLyricIdx] = useState(-1);
  const [playlists, setPlaylists] = useState<Playlist[] | null>(null);
  const [addedTo, setAddedTo] = useState<string | null>(null);
  const [recordingKey, setRecordingKey] = useState(false);
  const [settings, setSettings] = useState({
    hotkey: "Control+Alt+KeyS",
    always_on_top: true,
    pop_on_change: true,
    auto_hide_idle: true,
    voice_enabled: false,
    launch_at_login: false,
    accent: "#1db954",
    ui_scale: 1.0,
    island_opacity: 1.0,
  });
  const settingsRef = useRef(settings);
  settingsRef.current = settings;

  const islandRef = useRef<HTMLDivElement>(null);
  const seekFillRef = useRef<HTMLDivElement>(null);
  const seekCurRef = useRef<HTMLSpanElement>(null);
  const pillProgRef = useRef<HTMLDivElement>(null);
  const lyricLineRef = useRef<HTMLDivElement>(null);

  const visibleRef = useRef(false);
  const hoveredRef = useRef(false);
  const playingRef = useRef(false);
  const draggingRef = useRef(false);
  const winExpandedRef = useRef<boolean | null>(null);
  const prevTrackId = useRef<string | null | undefined>(undefined);
  const progressRef = useRef({ ms: 0, at: 0 });
  const lastVolSent = useRef<number | null>(null);
  const volumeRef = useRef<number | null>(null);
  const prevVol = useRef(50);
  const idleSince = useRef<number | null>(null);
  const peekTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  // True only when the island showed itself (track pop / music start) —
  // manual opens (tray, hotkey) never get idle-hidden.
  const autoShownRef = useRef(false);

  const collapseTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const volumeTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const popTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const bumpTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const trackId = playback?.item?.id;
  const isPlaying = playback?.is_playing ?? false;
  const forceExpanded = isLoggedIn === false;

  /* ---------- window sizing (async Rust-side; never sync-resize on Windows) ---------- */
  const resizeWindow = useCallback((big: boolean) => {
    if (winExpandedRef.current === big) return;
    winExpandedRef.current = big;
    void invoke("set_island_size", {
      expanded: big,
      uiScale: settingsRef.current.ui_scale,
    }).catch(() => {
      winExpandedRef.current = null;
    });
  }, []);

  // Scale change while visible → the window must grow/shrink with the
  // zoomed island or the bottom edge clips.
  useEffect(() => {
    winExpandedRef.current = null;
    resizeWindow(expanded);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [settings.ui_scale]);

  // Manual drag: data-tauri-drag-region swallows clicks on child buttons
  // (tauri-apps/tauri#9901), so we drag only from non-interactive areas.
  const handleDragStart = (e: React.MouseEvent) => {
    if (e.button !== 0) return;
    const t = e.target as HTMLElement;
    if (t.closest("button, input, a, .seek-track")) return;
    draggingRef.current = true;
    void appWindow.startDragging().catch(() => {
      draggingRef.current = false;
    });
  };

  const expand = useCallback((e?: React.MouseEvent) => {
    // If the OS drag ended outside our window the mouseup never reached us —
    // buttons-up on re-entry means the drag is over; recover cleanly.
    if (e && e.buttons === 0 && draggingRef.current) {
      draggingRef.current = false;
      void invoke("save_window_position").catch(() => {});
    }
    // Never expand mid-drag — resizing fights the OS drag loop (tauri#3990).
    if (draggingRef.current) return;
    hoveredRef.current = true;
    autoShownRef.current = false; // user touched it — treat as manual
    if (popTimer.current) {
      clearTimeout(popTimer.current);
      popTimer.current = null;
    }
    if (collapseTimer.current) {
      clearTimeout(collapseTimer.current);
      collapseTimer.current = null;
    }
    resizeWindow(true);
    setExpanded(true);
  }, [resizeWindow]);
  const expandRef = useRef(expand);
  expandRef.current = expand;

  // "Drop the bucket": the cursor is over the island when a drag ends, so
  // bloom back open immediately at the new spot instead of sitting collapsed
  // until the pointer leaves and re-enters.
  useEffect(() => {
    const onUp = () => {
      if (!draggingRef.current) return;
      draggingRef.current = false;
      void invoke("save_window_position").catch(() => {});
      expandRef.current();
    };
    window.addEventListener("mouseup", onUp);
    return () => window.removeEventListener("mouseup", onUp);
  }, []);

  const collapse = useCallback(() => {
    hoveredRef.current = false;
    setExpanded(false);
    setShowDevices(false);
    setShowSettings(false);
    setShowLyrics(false);
    setShowPlaylists(false);
    setShowChangelog(false);
    if (collapseTimer.current) clearTimeout(collapseTimer.current);
    collapseTimer.current = setTimeout(() => {
      // Never resize mid-drag — it fights the OS drag loop and freezes.
      if (!draggingRef.current) resizeWindow(false);
      collapseTimer.current = null;
    }, 340);
  }, [resizeWindow]);

  useEffect(() => {
    if (forceExpanded) {
      resizeWindow(true);
      setExpanded(true);
    }
  }, [forceExpanded, resizeWindow]);

  // Back to a pill once logged in (unless the pointer is on the island)
  useEffect(() => {
    if (isLoggedIn === true && !hoveredRef.current) collapse();
  }, [isLoggedIn, collapse]);

  // Collapse when the window loses focus — mouseleave can't fire once the
  // pointer has already left the window.
  useEffect(() => {
    const un = appWindow.onFocusChanged(({ payload }) => {
      if (!payload && expanded) collapse();
    });
    return () => {
      void un.then((f) => f());
    };
  }, [expanded, collapse]);

  /* ---------- playback fetch ---------- */
  const triggerBump = useCallback(() => {
    setBump(true);
    if (bumpTimer.current) clearTimeout(bumpTimer.current);
    bumpTimer.current = setTimeout(() => setBump(false), 550);
  }, []);

  const fetchPlayback = useCallback(async () => {
    try {
      const state = await invoke<PlaybackState | null>("get_current_playback");
      setPlayback(state);
      playingRef.current = !!state?.is_playing;
      setError("");

      // Re-sync volume if it changed elsewhere (Spotify app, another device)
      // — but not to a value we just sent and are waiting to be confirmed.
      if (
        state?.volume_percent != null &&
        state.volume_percent !== lastVolSent.current
      ) {
        setVolume(state.volume_percent);
        volumeRef.current = state.volume_percent;
      }

      const newId = state?.item?.id ?? null;
      const firstFetch = prevTrackId.current === undefined;
      const trackChanged =
        !firstFetch && newId !== null && newId !== prevTrackId.current;
      // Also pop when the app comes up / was idle-hidden and music starts.
      const musicStarted =
        (firstFetch || prevTrackId.current === null) &&
        newId !== null &&
        !!state?.is_playing;
      if (newId && (trackChanged || musicStarted) && settingsRef.current.pop_on_change) {
        if (visibleRef.current) {
          triggerBump();
        } else {
          // Dynamic Island move: pop in on a track change while hidden.
          autoShownRef.current = true;
          void invoke("show_window").catch(() => {});
          if (popTimer.current) clearTimeout(popTimer.current);
          popTimer.current = setTimeout(() => {
            if (!hoveredRef.current) void invoke("hide_window").catch(() => {});
          }, 5000);
        }
      }
      prevTrackId.current = newId;

      // Auto-hide when there's genuinely no session (Spotify closed, no
      // active device). Paused-with-track still counts as a session.
      if (!settingsRef.current.auto_hide_idle) {
        idleSince.current = null;
      } else if (!state?.item) {
        if (idleSince.current === null) {
          idleSince.current = Date.now();
        } else if (
          visibleRef.current &&
          autoShownRef.current &&
          !hoveredRef.current &&
          Date.now() - idleSince.current > 45_000
        ) {
          void invoke("hide_window").catch(() => {});
        }
      } else {
        idleSince.current = null;
      }
    } catch {
      // Transient poll failure (network blip, 429, timeout) — stay quiet;
      // a revoked session surfaces via the island-logout event instead.
    }
  }, [triggerBump]);
  const fetchPlaybackRef = useRef(fetchPlayback);
  fetchPlaybackRef.current = fetchPlayback;

  /* ---------- visibility events from Rust ---------- */
  useEffect(() => {
    const unlistenLogout = listen("island-logout", () => {
      prevTrackId.current = undefined;
      setIsLoggedIn(false);
      setPlayback(null);
    });
    // OS drags eat the final mouseup — Rust detects the end via Moved-event
    // debounce and emits this. Release = "drop the bucket" = bloom open.
    const unlistenDragEnd = listen("island-drag-end", () => {
      if (!draggingRef.current) return;
      draggingRef.current = false;
      void invoke("save_window_position").catch(() => {});
      expandRef.current();
    });
    const unlisten = listen<boolean>("island-visibility", (e) => {
      visibleRef.current = e.payload;
      if (e.payload) {
        // Fresh show: idle timer restarts so a manual open can't be
        // instantly auto-hidden by idle time accrued while hidden.
        idleSince.current = null;
        setPeek(true);
        if (peekTimer.current) clearTimeout(peekTimer.current);
        peekTimer.current = setTimeout(() => setPeek(false), 600);
        void fetchPlaybackRef.current();
      } else {
        hoveredRef.current = false;
        setExpanded(false);
        setShowDevices(false);
        setShowLyrics(false);
        setShowPlaylists(false);
        setShowChangelog(false);
        resizeWindow(false);
      }
    });
    // GSMTC doorbell: Windows says Spotify's state changed right now →
    // refetch immediately instead of waiting for the next scheduled poll.
    // This is what makes music-start and track-change pops instant.
    const unlistenSmtc = listen("smtc-change", () => {
      void fetchPlaybackRef.current();
    });
    return () => {
      void unlisten.then((f) => f());
      void unlistenLogout.then((f) => f());
      void unlistenDragEnd.then((f) => f());
      void unlistenSmtc.then((f) => f());
    };
  }, [resizeWindow]);

  /* ---------- auth ---------- */
  const checkAuth = useCallback(async () => {
    try {
      const tokens = await invoke<{ access_token: string } | null>(
        "get_spotify_tokens",
      );
      setIsLoggedIn(!!tokens);
    } catch {
      setIsLoggedIn(false);
    }
  }, []);

  useEffect(() => {
    checkAuth();
    invoke<typeof settings>("get_settings")
      .then((s) => setSettings((prev) => ({ ...prev, ...s })))
      .catch(() => {});
    invoke<string | null>("get_client_id")
      .then((id) => {
        if (id) {
          setClientId(id);
          setHasClientId(true);
        } else {
          setHasClientId(false);
        }
      })
      .catch(() => setHasClientId(false));
  }, [checkAuth]);

  /* ---------- polling: adaptive, slow-but-live while hidden ---------- */
  useEffect(() => {
    if (!isLoggedIn) return;
    let stopped = false;
    let timer: ReturnType<typeof setTimeout>;
    const tick = async () => {
      await fetchPlayback();
      if (!stopped) {
        // Hidden poll: faster while idle so "music started" pops instantly,
        // slower once a track is flowing (track-change detection only).
        const delay = !visibleRef.current
          ? playingRef.current
            ? 8000
            : 3000
          : playingRef.current
            ? 2000
            : 5000;
        timer = setTimeout(tick, delay);
      }
    };
    void tick();
    return () => {
      stopped = true;
      clearTimeout(timer);
    };
  }, [isLoggedIn, fetchPlayback]);

  /* ---------- progress: rAF + direct DOM writes (no React re-render churn) ---------- */
  useEffect(() => {
    if (!isSeeking) {
      progressRef.current = { ms: playback?.progress_ms ?? 0, at: Date.now() };
    }
    if (isSeeking) return; // drag handlers drive the bar while seeking

    const dur = playback?.item?.duration_ms ?? 0;
    const paint = () => {
      const { ms, at } = progressRef.current;
      let pos = ms;
      if (isPlaying && at) pos = ms + (Date.now() - at);
      if (dur > 0) pos = Math.min(pos, dur);
      const pct = dur > 0 ? Math.min(100, (pos / dur) * 100) : 0;
      if (seekFillRef.current) seekFillRef.current.style.width = `${pct}%`;
      if (pillProgRef.current) pillProgRef.current.style.width = `${pct}%`;
      if (seekCurRef.current) seekCurRef.current.textContent = fmtTime(pos);
    };
    paint();
    if (!isPlaying) return;

    let raf = 0;
    let last = 0;
    const step = (t: number) => {
      if (t - last >= 100) {
        last = t;
        paint();
      }
      raf = requestAnimationFrame(step);
    };
    raf = requestAnimationFrame(step);
    return () => cancelAnimationFrame(raf);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [playback, isSeeking, isPlaying]);

  /* ---------- saved-track (heart) ---------- */
  useEffect(() => {
    if (!isLoggedIn || !trackId) {
      setSaved(false);
      return;
    }
    invoke<boolean>("is_track_saved", { trackId })
      .then(setSaved)
      .catch(() => setSaved(false));
  }, [isLoggedIn, trackId]);

  /* ---------- up next (only fetched while expanded) ---------- */
  useEffect(() => {
    if (!expanded || !isLoggedIn || !trackId) {
      setUpNext(undefined);
      return;
    }
    invoke<Track | null>("get_up_next")
      .then(setUpNext)
      .catch(() => setUpNext(null));
  }, [expanded, isLoggedIn, trackId]);

  /* ---------- album-art tint ---------- */
  const albumArt = playback?.item?.album?.images?.[0]?.url ?? "";
  const tintArt =
    playback?.item?.album?.images?.[playback.item.album.images.length - 1]?.url ??
    "";
  useEffect(() => {
    const island = islandRef.current;
    if (!tintArt || !island) return;
    const img = new Image();
    img.crossOrigin = "anonymous";
    img.src = tintArt;
    img.onload = () => {
      try {
        const c = document.createElement("canvas");
        c.width = c.height = 1;
        const ctx = c.getContext("2d", { willReadFrequently: true });
        if (!ctx) return;
        ctx.drawImage(img, 0, 0, 1, 1);
        const [r, g, b] = ctx.getImageData(0, 0, 1, 1).data;
        island.style.setProperty("--tint", `rgb(${r}, ${g}, ${b})`);
      } catch {
        /* canvas tainted — keep default tint */
      }
    };
  }, [tintArt]);

  /* ---------- actions ---------- */
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

  const handleLogin = async () => {
    try {
      const url = await invoke<string>("start_spotify_login");
      await openUrl(url);
      let attempts = 0;
      const poll = setInterval(async () => {
        attempts++;
        const tokens = await invoke("get_spotify_tokens").catch(() => null);
        if (tokens || attempts > 30) {
          clearInterval(poll);
          if (tokens) setIsLoggedIn(true);
        }
      }, 2000);
    } catch (e) {
      if (typeof e === "string") setError(e);
    }
  };

  const handleLogout = async () => {
    await invoke("logout_spotify");
    prevTrackId.current = undefined;
    setIsLoggedIn(false);
    setPlayback(null);
  };

  const handleControl = async (action: string) => {
    try {
      if (action === "play" || action === "pause") {
        playingRef.current = action === "play";
        setPlayback((p) => (p ? { ...p, is_playing: action === "play" } : p));
      }
      await invoke("control_playback", { action });
      setTimeout(fetchPlayback, 350);
    } catch (e) {
      // Optimistic toggle didn't take — resync with reality.
      void fetchPlayback();
      if (typeof e === "string") setError(e);
    }
  };

  const toggleShuffle = async () => {
    if (!playback) return;
    const next = !playback.shuffle_state;
    setPlayback({ ...playback, shuffle_state: next });
    try {
      await invoke("set_shuffle", { state: next });
    } catch (e) {
      if (typeof e === "string") setError(e);
    }
  };

  const cycleRepeat = async () => {
    if (!playback) return;
    const order = ["off", "context", "track"];
    const next = order[(order.indexOf(playback.repeat_state) + 1) % 3];
    setPlayback({ ...playback, repeat_state: next });
    try {
      await invoke("set_repeat", { state: next });
    } catch (e) {
      if (typeof e === "string") setError(e);
    }
  };

  const toggleSaved = async () => {
    if (!trackId) return;
    const next = !saved;
    setSaved(next);
    try {
      await invoke("set_track_saved", { trackId, saved: next });
    } catch (e) {
      setSaved(!next);
      if (typeof e === "string") setError(e);
    }
  };

  const handleVolume = useCallback(
    (v: number) => {
      setVolume(v);
      volumeRef.current = v;
      lastVolSent.current = v;
      if (volumeTimer.current) clearTimeout(volumeTimer.current);
      volumeTimer.current = setTimeout(() => {
        invoke("set_volume", { volumePercent: v }).catch(() => {
          lastVolSent.current = null;
        });
      }, 180);
    },
    [],
  );

  // Scroll on the island = volume.
  const handleWheel = (e: React.WheelEvent) => {
    const cur = volumeRef.current ?? playback?.volume_percent;
    if (cur == null) return;
    handleVolume(Math.max(0, Math.min(100, cur + (e.deltaY < 0 ? 4 : -4))));
  };

  const handleContextMenu = (e: React.MouseEvent) => {
    e.preventDefault();
    void invoke("hide_window");
  };

  const openInSpotify = () => {
    const url = playback?.item?.external_urls?.spotify;
    if (url) void openUrl(url);
  };

  /* ---------- settings ---------- */
  const updateSetting = (
    key: keyof typeof settings,
    value: boolean | string | number,
  ) => {
    setSettings((s) => ({ ...s, [key]: value }));
    invoke("update_setting", { key, value }).catch(() => {});
  };

  // "Press any combo" — captures into global-hotkey's accelerator format.
  useEffect(() => {
    if (!recordingKey) return;
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Escape") {
        setRecordingKey(false);
        return;
      }
      const code = e.code;
      if (/^(Control|Alt|Shift|Meta)(Left|Right)$/.test(code)) return;
      const mods: string[] = [];
      if (e.ctrlKey) mods.push("Control");
      if (e.altKey) mods.push("Alt");
      if (e.shiftKey) mods.push("Shift");
      if (e.metaKey) mods.push("Meta");
      const accel = [...mods, code].join("+");
      invoke("set_hotkey", { accelerator: accel })
        .then(() => {
          setSettings((s) => ({ ...s, hotkey: accel }));
          setRecordingKey(false);
        })
        .catch((err) => {
          setError(typeof err === "string" ? err : "Shortcut unavailable");
          setRecordingKey(false);
        });
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [recordingKey]);

  // Escape closes open sheets (unless we're mid hotkey-recording, where
  // Esc cancels the recorder instead).
  useEffect(() => {
    if (!showSettings && !showDevices && !showLyrics && !showPlaylists && !showChangelog) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !recordingKey) {
        setShowSettings(false);
        setShowDevices(false);
        setShowLyrics(false);
        setShowPlaylists(false);
        setShowChangelog(false);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [showSettings, showDevices, showLyrics, showPlaylists, showChangelog, recordingKey]);

  const fmtHotkey = (accel: string) =>
    accel
      .split("+")
      .map((t) =>
        t.replace(/^Key/, "").replace(/^Digit/, "").replace(/^Arrow/, ""),
      )
      .map((t) => (t === "Control" ? "Ctrl" : t === "Meta" ? "Win" : t))
      .join(" + ");

  const toggleMute = () => {
    const cur = volumeRef.current ?? playback?.volume_percent ?? 0;
    if (cur > 0) {
      prevVol.current = cur;
      handleVolume(0);
    } else {
      handleVolume(prevVol.current || 50);
    }
  };

  const copyLink = async () => {
    const url = playback?.item?.external_urls?.spotify;
    if (!url) return;
    try {
      await navigator.clipboard.writeText(url);
      setCopied(true);
      setTimeout(() => setCopied(false), 1200);
    } catch {
      /* clipboard unavailable */
    }
  };

  const openDevices = async () => {
    if (showDevices) {
      setShowDevices(false);
      return;
    }
    setShowDevices(true);
    try {
      setDevices(await invoke<Device[]>("get_devices"));
    } catch (e) {
      setShowDevices(false);
      if (typeof e === "string") setError(e);
    }
  };

  const pickDevice = async (d: Device) => {
    if (!d.id) return;
    setShowDevices(false);
    try {
      await invoke("transfer_playback", { deviceId: d.id });
      setTimeout(fetchPlayback, 500);
    } catch (e) {
      if (typeof e === "string") setError(e);
    }
  };

  /* ---------- synced lyrics (LRClib — free, no key) ---------- */
  // Timed lines when LRC data exists, otherwise one block of plain lines.
  useEffect(() => {
    if (!showLyrics) return;
    const item = playback?.item;
    if (!item) {
      setLyrics(null);
      return;
    }
    setLyrics("loading");
    const params = new URLSearchParams({
      track_name: item.name,
      artist_name: item.artists[0]?.name ?? "",
      album_name: item.album.name,
      duration: String(Math.round(item.duration_ms / 1000)),
    });
    let dead = false;
    fetch(`https://lrclib.net/api/get?${params}`)
      .then((r) => (r.ok ? r.json() : null))
      .then((d) => {
        if (dead) return;
        const raw: string | null = d?.syncedLyrics ?? d?.plainLyrics ?? null;
        setLyrics(raw ? parseLrc(raw) : null);
      })
      .catch(() => {
        if (!dead) setLyrics(null);
      });
    return () => {
      dead = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [showLyrics, trackId]);

  // Karaoke cursor: walks progressRef (same clock as the seek bar) to the
  // current line, then scrolls it into view. Only runs while open.
  const lyricsSynced =
    Array.isArray(lyrics) && lyrics.some((l) => l.t > 0);
  useEffect(() => {
    if (!showLyrics || !lyricsSynced || !Array.isArray(lyrics)) return;
    const iv = setInterval(() => {
      const { ms, at } = progressRef.current;
      const pos = playingRef.current && at ? ms + (Date.now() - at) : ms;
      let idx = -1;
      for (let i = 0; i < lyrics.length; i++) {
        if (lyrics[i].t <= pos) idx = i;
        else break;
      }
      setLyricIdx(idx);
    }, 400);
    return () => clearInterval(iv);
  }, [showLyrics, lyricsSynced, lyrics]);

  // Keep the active lyric centered in the sheet.
  useEffect(() => {
    if (lyricIdx < 0) return;
    lyricLineRef.current
      ?.querySelector(".lyric-line.cur")
      ?.scrollIntoView({ block: "center", behavior: "smooth" });
  }, [lyricIdx]);

  /* ---------- save to playlist ---------- */
  const openPlaylists = async () => {
    if (showPlaylists) {
      setShowPlaylists(false);
      return;
    }
    setShowPlaylists(true);
    try {
      setPlaylists(await invoke<Playlist[]>("get_playlists"));
    } catch (e) {
      setShowPlaylists(false);
      if (typeof e === "string") setError(e);
    }
  };

  const saveToPlaylist = async (p: Playlist) => {
    if (!trackId) return;
    try {
      await invoke("save_to_playlist", { playlistId: p.id, trackId });
      setAddedTo(p.id);
      setTimeout(() => {
        setAddedTo(null);
        setShowPlaylists(false);
      }, 900);
    } catch (e) {
      if (typeof e === "string") setError(e);
    }
  };

  /* ---------- seek ---------- */
  const startSeekDrag = (e: React.MouseEvent<HTMLDivElement>) => {
    if (!playback?.item) return;
    e.preventDefault();
    const track = e.currentTarget;
    const duration = playback.item.duration_ms;
    setIsSeeking(true);

    const getRatio = (clientX: number) => {
      const rect = track.getBoundingClientRect();
      return Math.max(0, Math.min(1, (clientX - rect.left) / rect.width));
    };
    const update = (clientX: number) => {
      const ratio = getRatio(clientX);
      const pos = ratio * duration;
      const pct = `${ratio * 100}%`;
      if (seekFillRef.current) seekFillRef.current.style.width = pct;
      if (pillProgRef.current) pillProgRef.current.style.width = pct;
      if (seekCurRef.current) seekCurRef.current.textContent = fmtTime(pos);
    };
    update(e.clientX);

    const onMove = (ev: MouseEvent) => update(ev.clientX);
    const onUp = async (ev: MouseEvent) => {
      document.removeEventListener("mousemove", onMove);
      document.removeEventListener("mouseup", onUp);
      const positionMs = Math.floor(getRatio(ev.clientX) * duration);
      // Optimistic: jump the bar immediately, then let the API confirm.
      setPlayback((p) => (p ? { ...p, progress_ms: positionMs } : p));
      setIsSeeking(false);
      try {
        await invoke("seek_playback", { positionMs });
        setTimeout(fetchPlayback, 350);
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

  /* ---------- render ---------- */
  const artistNames = playback?.item?.artists.map((a) => a.name).join(", ") ?? "";
  const curVol = volume ?? playback?.volume_percent ?? 0;
  const VolIcon = curVol === 0 ? VolumeX : curVol < 50 ? Volume1 : Volume2;

  const islandCls = [
    "island",
    expanded ? "expanded" : "",
    isPlaying ? "playing" : "",
    bump ? "bump" : "",
    peek ? "peek" : "",
  ]
    .filter(Boolean)
    .join(" ");

  // Customization — accent drives --accent/--accent-hi/--accent-glow,
  // zoom scales without fighting the transform animations.
  const accentHi =
    ACCENTS.find(([a]) => a === settings.accent)?.[1] ?? settings.accent;
  const islandStyle = {
    "--accent": settings.accent,
    "--accent-hi": accentHi,
    "--accent-glow": `color-mix(in srgb, ${settings.accent} 40%, transparent)`,
    zoom: settings.ui_scale,
    opacity: settings.island_opacity,
  } as React.CSSProperties;

  /* ----- loading ----- */
  if (isLoggedIn === null) {
    return (
      <div className="stage">
        <div className="island" onMouseDown={handleDragStart}>
          <div className="face face-collapsed center">
            <span className="spinner" />
            <span className="loading-text">SpotPeek</span>
          </div>
        </div>
      </div>
    );
  }

  /* ----- login ----- */
  if (!isLoggedIn) {
    return (
      <div className="stage">
        <div className="island expanded" onMouseDown={handleDragStart}>
          <button className="top-close" onClick={handleHide} aria-label="Hide">
            <X size={13} />
          </button>
          <div className="face face-expanded login-face">
            <div className="brand">
              <div className="brand-icon">
                <AudioLines size={19} />
              </div>
              <div className="brand-text">
                <span className="brand-title">SpotPeek</span>
                <span className="brand-sub">Your Spotify island</span>
              </div>
            </div>

            {error && <p className="error-text">{error}</p>}

            {hasClientId === false ? (
              <div className="client-id-form">
                <input
                  type="text"
                  className="client-id-input"
                  placeholder="Spotify Client ID"
                  value={clientId}
                  onChange={(e) => setClientId(e.currentTarget.value)}
                  onKeyDown={(e) => e.key === "Enter" && saveClientId()}
                />
                <button className="btn-primary" onClick={saveClientId}>
                  Save Client ID
                </button>
                <p className="hint">developer.spotify.com/dashboard</p>
              </div>
            ) : (
              <button className="btn-primary" onClick={handleLogin}>
                <LogIn size={15} />
                Connect Spotify
              </button>
            )}

            <div className="login-credit">
              made by
              <button onClick={() => void openUrl("https://ko-fi.com/w0wzahh")}>
                w0wzahh
                <Coffee size={9} />
              </button>
            </div>
          </div>
        </div>
      </div>
    );
  }

  /* ----- player ----- */
  return (
    <div className="stage">
      <div
        className={islandCls}
        style={islandStyle}
        ref={islandRef}
        onMouseEnter={expand}
        onMouseLeave={collapse}
        onMouseDown={handleDragStart}
        onContextMenu={handleContextMenu}
      >
        {/* collapsed face */}
        <div className="face face-collapsed">
          <div
            className={`mini-art ${albumArt ? "clickable" : ""}`}
            onClick={openInSpotify}
          >
            {albumArt ? (
              <img key={albumArt} src={albumArt} alt="" draggable={false} />
            ) : (
              <Music size={15} />
            )}
          </div>
          <div className="mini-meta">
            <Marquee
              className="mini-title"
              text={playback?.item?.name || "Nothing playing"}
            />
            <Marquee
              className="mini-artist"
              text={artistNames || "Open Spotify and play something"}
            />
          </div>
          <EqBars active={isPlaying} />
          <button
            className="mini-play"
            onClick={() => handleControl(isPlaying ? "pause" : "play")}
            aria-label={isPlaying ? "Pause" : "Play"}
          >
            {isPlaying ? <Pause size={14} /> : <Play size={14} />}
          </button>
          <div className="pill-progress" ref={pillProgRef} />
        </div>

        {/* expanded face */}
        <div className="face face-expanded">
          <div className="row-top">
            <div
              className={`art ${albumArt ? "clickable" : ""}`}
              onClick={openInSpotify}
            >
              {albumArt ? (
                <img key={albumArt} src={albumArt} alt="" draggable={false} />
              ) : (
                <Music size={22} />
              )}
              {isPlaying && <span className="playing-dot" />}
            </div>
            <div className="meta">
              <Marquee
                className="track-name"
                text={playback?.item?.name || "Nothing playing"}
              />
              <Marquee
                className="artist-name"
                text={artistNames || "Open Spotify and play something"}
              />
            </div>
            <button
              className={`heart-btn ${saved ? "saved" : ""}`}
              onClick={toggleSaved}
              aria-label="Like"
            >
              <Heart size={15} fill={saved ? "currentColor" : "none"} />
            </button>
          </div>

          <div className="seek-row">
            <span className="seek-time" ref={seekCurRef}>
              0:00
            </span>
            <div className="seek-track" onMouseDown={startSeekDrag}>
              <div className="seek-fill" ref={seekFillRef} />
            </div>
            <span className="seek-time">
              {fmtTime(playback?.item?.duration_ms ?? 0)}
            </span>
          </div>

          <div className="controls-row">
            <button
              className={`ctl side ${playback?.shuffle_state ? "active" : ""}`}
              onClick={toggleShuffle}
              aria-label="Shuffle"
            >
              <Shuffle size={14} />
            </button>
            <button
              className="ctl"
              onClick={() => handleControl("previous")}
              aria-label="Previous"
            >
              <SkipBack size={16} />
            </button>
            <button
              className="ctl main"
              onClick={() => handleControl(isPlaying ? "pause" : "play")}
              aria-label={isPlaying ? "Pause" : "Play"}
            >
              {isPlaying ? <Pause size={17} /> : <Play size={17} />}
            </button>
            <button
              className="ctl"
              onClick={() => handleControl("next")}
              aria-label="Next"
            >
              <SkipForward size={16} />
            </button>
            <button
              className={`ctl side ${playback?.repeat_state !== "off" ? "active" : ""}`}
              onClick={cycleRepeat}
              aria-label="Repeat"
            >
              {playback?.repeat_state === "track" ? (
                <Repeat1 size={14} />
              ) : (
                <Repeat size={14} />
              )}
            </button>
          </div>

          <div className="bottom-row">
            <button
              className="vol-btn"
              onClick={toggleMute}
              onWheel={handleWheel}
              aria-label={curVol === 0 ? "Unmute" : "Mute"}
            >
              <VolIcon size={13} className="vol-icon" />
            </button>
            <input
              type="range"
              className="vol-slider"
              onWheel={handleWheel}
              min={0}
              max={100}
              value={volume ?? playback?.volume_percent ?? 0}
              onChange={(e) => handleVolume(Number(e.currentTarget.value))}
            />
            <div className="row-actions">
              {playback?.device?.name && (
                <button
                  className="mini-btn"
                  onClick={openDevices}
                  aria-label={`Play on ${playback.device.name}`}
                  title={playback.device.name}
                >
                  <Speaker size={11} />
                </button>
              )}
              {playback?.item?.external_urls?.spotify && (
                <button
                  className={`mini-btn ${copied ? "copied" : ""}`}
                  onClick={copyLink}
                  aria-label="Copy track link"
                  title="Copy link"
                >
                  {copied ? <Check size={11} /> : <Link2 size={11} />}
                </button>
              )}
              {playback?.item && (
                <button
                  className={`mini-btn ${showLyrics ? "active" : ""}`}
                  onClick={() => setShowLyrics((s) => !s)}
                  aria-label="Lyrics"
                  title="Lyrics"
                >
                  <BookOpenText size={11} />
                </button>
              )}
              {playback?.item && (
                <button
                  className={`mini-btn ${showPlaylists ? "active" : ""}`}
                  onClick={openPlaylists}
                  aria-label="Save to playlist"
                  title="Save to playlist"
                >
                  <ListPlus size={11} />
                </button>
              )}
              <button
                className={`mini-btn ${showSettings ? "active" : ""}`}
                onClick={() => setShowSettings((s) => !s)}
                aria-label="Settings"
                title="Settings"
              >
                <Settings size={11} />
              </button>
            </div>
          </div>

          {upNext !== undefined && (
            <div className="up-next">
              <ListMusic size={11} className="up-icon" />
              <span className="up-label">Next</span>
              <Marquee
                className="up-title"
                text={
                  upNext
                    ? `${upNext.name} — ${upNext.artists.map((a) => a.name).join(", ")}`
                    : "Nothing queued up"
                }
              />
            </div>
          )}

          {/* device picker sheet */}
          {showDevices && (
            <div className="device-sheet" onClick={() => setShowDevices(false)}>
              <div className="sheet-title">
                <button
                  className="sheet-back"
                  onClick={(e) => {
                    e.stopPropagation();
                    setShowDevices(false);
                  }}
                  aria-label="Back"
                >
                  <ChevronLeft size={13} />
                </button>
                <span>Play on</span>
              </div>
              {(devices ?? []).map((d) => (
                <button
                  key={d.id ?? d.name}
                  className={`device-item ${d.is_active || d.name === playback?.device?.name ? "cur" : ""}`}
                  onClick={(e) => {
                    e.stopPropagation();
                    void pickDevice(d);
                  }}
                >
                  <Speaker size={13} />
                  <span className="dev-name">{d.name}</span>
                  <span className="dev-type">{d.device_type}</span>
                </button>
              ))}
              {devices === null && <span className="hint">Loading…</span>}
              {devices && devices.length === 0 && (
                <span className="hint">No devices found — open Spotify somewhere</span>
              )}
            </div>
          )}

          {/* settings sheet */}
          {showSettings && (
            <div
              className="device-sheet settings-sheet"
              onClick={(e) => {
                if (e.target === e.currentTarget) setShowSettings(false);
              }}
            >
              <div className="sheet-title">
                <button
                  className="sheet-back"
                  onClick={(e) => {
                    e.stopPropagation();
                    setShowSettings(false);
                  }}
                  aria-label="Back"
                >
                  <ChevronLeft size={13} />
                </button>
                <span>Settings</span>
              </div>

              <div className="set-row">
                <span className="set-label">Toggle hotkey</span>
                <button
                  className={`key-chip ${recordingKey ? "recording" : ""}`}
                  onClick={() => setRecordingKey((r) => !r)}
                >
                  {recordingKey ? "Press keys…" : fmtHotkey(settings.hotkey)}
                </button>
              </div>

              {(
                [
                  ["always_on_top", "Always on top"],
                  ["pop_on_change", "Pop in on track change"],
                  ["auto_hide_idle", "Hide when nothing plays"],
                  ["launch_at_login", "Launch at login"],
                  ["voice_enabled", "Voice commands"],
                ] as const
              ).map(([key, label]) => (
                <div className="set-row" key={key}>
                  <span className="set-label">
                    {key === "voice_enabled" && <Mic size={10} />}
                    {label}
                  </span>
                  <button
                    className={`toggle ${settings[key] ? "on" : ""}`}
                    onClick={() => updateSetting(key, !settings[key])}
                    aria-label={label}
                  >
                    <span className="knob" />
                  </button>
                </div>
              ))}

              {settings.voice_enabled && (
                <span className="hint voice-hint">
                  peek · hide · play · pause · next · previous · like
                </span>
              )}

              <div className="set-row">
                <span className="set-label">Accent</span>
                <div className="swatches">
                  {ACCENTS.map(([a]) => (
                    <button
                      key={a}
                      className={`swatch ${settings.accent === a ? "cur" : ""}`}
                      style={{ background: a }}
                      onClick={() => updateSetting("accent", a)}
                      aria-label={`Accent ${a}`}
                    />
                  ))}
                </div>
              </div>

              <div className="set-row">
                <span className="set-label">Island size</span>
                <div className="seg">
                  {(
                    [
                      ["S", 0.85],
                      ["M", 1],
                      ["L", 1.15],
                    ] as const
                  ).map(([label, v]) => (
                    <button
                      key={label}
                      className={`seg-btn ${settings.ui_scale === v ? "cur" : ""}`}
                      onClick={() => updateSetting("ui_scale", v)}
                    >
                      {label}
                    </button>
                  ))}
                </div>
              </div>

              <div className="set-row">
                <span className="set-label">Opacity</span>
                <input
                  type="range"
                  className="vol-slider set-slider"
                  min={55}
                  max={100}
                  value={Math.round(settings.island_opacity * 100)}
                  onChange={(e) =>
                    updateSetting(
                      "island_opacity",
                      Number(e.currentTarget.value) / 100,
                    )
                  }
                />
              </div>

              <div className="set-row">
                <span className="set-label">Snap position</span>
                <div className="seg">
                  {(["top-left", "top-center", "top-right"] as const).map(
                    (c) => (
                      <button
                        key={c}
                        className="seg-btn"
                        onClick={() =>
                          invoke("snap_island", { corner: c }).catch(() => {})
                        }
                      >
                        {c.replace("top-", "")}
                      </button>
                    ),
                  )}
                </div>
              </div>

              <div className="set-row">
                <button
                  className="set-btn"
                  onClick={() =>
                    invoke("reset_island_position").catch(() => {})
                  }
                >
                  <RotateCcw size={10} />
                  Reset position
                </button>
                <button className="set-btn danger" onClick={handleLogout}>
                  <LogOut size={10} />
                  Disconnect
                </button>
              </div>

            </div>
          )}

          {/* lyrics sheet */}
          {showLyrics && (
            <div
              className="device-sheet lyrics-sheet"
              ref={lyricLineRef}
              onClick={() => setShowLyrics(false)}
            >
              <div className="sheet-title">
                <button
                  className="sheet-back"
                  onClick={(e) => {
                    e.stopPropagation();
                    setShowLyrics(false);
                  }}
                  aria-label="Back"
                >
                  <ChevronLeft size={13} />
                </button>
                <span>Lyrics</span>
              </div>
              <div className="lyric-body">
                {lyrics === "loading" && <span className="hint">Loading…</span>}
                {lyrics === null && (
                  <span className="hint">
                    {playback?.item
                      ? "No lyrics found for this track"
                      : "Play something first"}
                  </span>
                )}
                {Array.isArray(lyrics) &&
                  lyrics.map((l, i) => (
                    <p
                      key={i}
                      className={`lyric-line ${lyricsSynced && i === lyricIdx ? "cur" : ""}`}
                    >
                      {l.text}
                    </p>
                  ))}
              </div>
            </div>
          )}

          {/* save-to-playlist sheet */}
          {showPlaylists && (
            <div
              className="device-sheet"
              onClick={() => setShowPlaylists(false)}
            >
              <div className="sheet-title">
                <button
                  className="sheet-back"
                  onClick={(e) => {
                    e.stopPropagation();
                    setShowPlaylists(false);
                  }}
                  aria-label="Back"
                >
                  <ChevronLeft size={13} />
                </button>
                <span>Save to…</span>
              </div>
              {(playlists ?? []).map((p) => (
                <button
                  key={p.id}
                  className="device-item"
                  onClick={(e) => {
                    e.stopPropagation();
                    void saveToPlaylist(p);
                  }}
                >
                  {p.image ? (
                    <img className="pl-art" src={p.image} alt="" draggable={false} />
                  ) : (
                    <ListMusic size={13} />
                  )}
                  <span className="dev-name">{p.name}</span>
                  <span className="dev-type">
                    {addedTo === p.id ? "Added ✓" : `${p.track_count} tracks`}
                  </span>
                </button>
              ))}
              {playlists === null && <span className="hint">Loading…</span>}
              {playlists && playlists.length === 0 && (
                <span className="hint">No playlists found</span>
              )}
            </div>
          )}

          {/* changelog sheet */}
          {showChangelog && (
            <div
              className="device-sheet changelog-sheet"
              onClick={() => setShowChangelog(false)}
            >
              <div className="sheet-title">
                <button
                  className="sheet-back"
                  onClick={(e) => {
                    e.stopPropagation();
                    setShowChangelog(false);
                  }}
                  aria-label="Back"
                >
                  <ChevronLeft size={13} />
                </button>
                <span>What's new</span>
              </div>
              <div className="seg cl-filter">
                {["all", ...CHANGELOG.map((r) => r.version)].map((v) => (
                  <button
                    key={v}
                    className={`seg-btn ${clFilter === v ? "cur" : ""}`}
                    onClick={(e) => {
                      e.stopPropagation();
                      setClFilter(v);
                    }}
                  >
                    {v === "all" ? "All" : `v${v}`}
                  </button>
                ))}
              </div>
              {CHANGELOG.filter(
                (rel) => clFilter === "all" || rel.version === clFilter,
              ).map((rel) => (
                <div className="cl-rel" key={rel.version}>
                  <span className="cl-ver">v{rel.version}</span>
                  {rel.sections.map(([cat, items]) => (
                    <div className="cl-cat" key={cat}>
                      <span className={`cl-tag ${cat.toLowerCase()}`}>
                        {cat}
                      </span>
                      {items.map((item) => (
                        <p className="cl-item" key={item}>
                          {item}
                        </p>
                      ))}
                    </div>
                  ))}
                </div>
              ))}
            </div>
          )}

          {error && <p className="error-text float">{error}</p>}

          <div className="credit-row">
            <button
              className="credit"
              onClick={() => void openUrl("https://ko-fi.com/w0wzahh")}
              aria-label="w0wzahh on Ko-fi"
            >
              w0wzahh
              <Coffee size={8} />
            </button>
            <button
              className="credit-icon"
              onClick={() => void openUrl("https://github.com/w0wzahh")}
              aria-label="GitHub"
              title="GitHub"
            >
              <Github size={9} />
            </button>
            <button
              className="credit-icon"
              onClick={() =>
                void openUrl(
                  "mailto:emrebelgrad@gmail.com?subject=SpotPeek%20bug%20report",
                )
              }
              aria-label="Report a bug"
              title="Report a bug"
            >
              <Bug size={9} />
            </button>
            <button
              className={`credit-icon ${showChangelog ? "active" : ""}`}
              onClick={() => setShowChangelog((s) => !s)}
              aria-label="What's new"
              title="What's new"
            >
              <History size={9} />
            </button>
          </div>
        </div>

        {!(showDevices || showSettings || showLyrics || showPlaylists || showChangelog) && (
          <div className="top-actions">
            <button className="icon-btn" onClick={handleLogout} aria-label="Logout">
              <LogOut size={12} />
            </button>
            <button className="icon-btn" onClick={handleHide} aria-label="Hide">
              <X size={12} />
            </button>
          </div>
        )}
      </div>
    </div>
  );
}

export default App;
