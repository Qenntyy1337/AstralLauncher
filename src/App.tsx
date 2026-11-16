import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { api, formatBytes, formatSpeed, formatEta } from "./api";
import { setLang, t } from "./i18n";
import { sounds } from "./sounds";
import type { Config, ConsoleEntry, ProgressPayload, TaskFinished, LaunchInfo, GameExitEvent } from "./types";import {
  IconPlay,
  IconMods,
  IconSettings,
  IconConsole,
  IconCancel,
  LogoMark,
  IconCheck,
  IconError,
  IconUser,
} from "./icons";
import PlayTab from "./components/PlayTab";
import ModsTab from "./components/ModsTab";
import SettingsTab from "./components/SettingsTab";
import ConsoleTab from "./components/ConsoleTab";
import { ProgressBar } from "./components/common";

type TabId = "play" | "mods" | "settings" | "console";

interface Toast {
  id: number;
  msg: string;
  ok: boolean;
}

const TABS: { id: TabId; label: string; icon: React.ReactNode }[] = [
  { id: "play", label: "tab.play", icon: <IconPlay size={21} /> },
  { id: "mods", label: "tab.mods", icon: <IconMods size={21} /> },
  { id: "settings", label: "tab.settings", icon: <IconSettings size={21} /> },
  { id: "console", label: "tab.console", icon: <IconConsole size={21} /> },
];

const ACCENTS: Record<string, { a: string; h: string; a2: string }> = {
  violet: { a: "#7b6cff", h: "#8f80ff", a2: "#3ec5ff" },
  blue: { a: "#4f8bff", h: "#6b9dff", a2: "#37e0ff" },
  green: { a: "#2ec27e", h: "#4fe39a", a2: "#7de0ff" },
  pink: { a: "#e35fb2", h: "#f07fc6", a2: "#8f9dff" },
  amber: { a: "#d99a3d", h: "#f0b558", a2: "#ff9d6b" },
};

export default function App() {
  const [config, setConfig] = useState<Config | null>(null);
  const [launcherVersion, setLauncherVersion] = useState("");
  const [tab, setTab] = useState<TabId>("play");
  const [logs, setLogs] = useState<ConsoleEntry[]>([]);
  const [jobs, setJobs] = useState<Record<string, ProgressPayload>>({});
  const [toasts, setToasts] = useState<Toast[]>([]);
  const [refreshSignal, setRefreshSignal] = useState(0);
  const [customWallpaperData, setCustomWallpaperData] = useState<string | null>(null);
  const [running, setRunning] = useState<LaunchInfo | null>(null);
  const [sessionStart, setSessionStart] = useState<number>(0);
  const [sessionNow, setSessionNow] = useState<number>(0);
  const [crash, setCrash] = useState<GameExitEvent | null>(null);
  const [dragOver, setDragOver] = useState(false);
  const [accountMenu, setAccountMenu] = useState(false);
  const accountMenuRef = useRef<HTMLDivElement | null>(null);
  const cfgRef = useRef<Config | null>(null);

  useEffect(() => {
    cfgRef.current = config;
  }, [config]);

  const showToast = useCallback((msg: string, ok = true) => {
    const id = Date.now() + Math.random();
    setToasts((prev) => [...prev.slice(-4), { id, msg, ok }]);
    window.setTimeout(() => {
      setToasts((prev) => prev.filter((t) => t.id !== id));
    }, 5000);
  }, []);

  const patchConfig = useCallback(
    async (patch: Partial<Config>) => {
      setConfig((prev) => {
        if (!prev) return prev;
        const updated = { ...prev, ...patch };
        api
          .updateConfig(updated)
          .then((cfg) => setConfig(cfg))
          .catch((e) => showToast(String(e), false));
        return updated;
      });
    },
    [showToast],
  );

  const startJob = useCallback((jobId: string, label: string) => {
    setJobs((prev) => ({
      ...prev,
      [jobId]: {
        job_id: jobId,
        label,
        phase: "prepare",
        total_bytes: 0,
        downloaded_bytes: 0,
        remaining_bytes: 0,
        files_done: 0,
        files_total: 0,
        current_file: "",
        speed_bps: 0,
      },
    }));
  }, []);

  const logConsole = useCallback((entry: ConsoleEntry) => {
    setLogs((prev) => {
      const next = [...prev, entry];
      return next.length > 4000 ? next.slice(-4000) : next;
    });
  }, []);

  useEffect(() => {
    api
      .getConfig()
      .then((cfg) => {
        setConfig(cfg);
        api
          .getLauncherVersion()
          .then(setLauncherVersion)
          .catch(() => setLauncherVersion("1.0.1"));
        logConsole({
          ts: new Date().toLocaleTimeString("ru-RU", { hour12: false }),
          level: "info",
          source: "launcher",
          message: `AstralLauncher запущен. Каталог: ${cfg.root_dir}`,
        });
      })
      .catch((e) => showToast(String(e), false));

    const unlisteners: Promise<() => void>[] = [
      listen<ProgressPayload>("download-progress", (e) => {
        const p = e.payload;
        setJobs((prev) => (prev[p.job_id] ? { ...prev, [p.job_id]: p } : prev));
      }),
      listen<TaskFinished>("task-finished", (e) => {
        const t = e.payload;
        setJobs((prev) => {
          const next = { ...prev };
          delete next[t.job_id];
          return next;
        });
        showToast(t.message, t.ok);
        if (cfgRef.current?.sounds_enabled) {
          t.ok ? sounds.success() : sounds.error();
        }
        logConsole({
          ts: new Date().toLocaleTimeString("ru-RU", { hour12: false }),
          level: t.ok ? "info" : "error",
          source: "task",
          message: t.message,
        });
        if (t.ok && (t.kind === "install-version" || t.kind === "install-content" || t.kind === "install-java" || t.kind === "install-mrpack")) {
          setRefreshSignal((n) => n + 1);
          // после установки бэкенд делает версию активной — подтягиваем конфиг
          if (t.kind === "install-version") {
            api.getConfig().then(setConfig).catch(() => {});
          }
        }
      }),
      listen<ConsoleEntry>("console-log", (e) => logConsole(e.payload)),
      listen<GameExitEvent>("game-exit", (e) => {
        const p = e.payload;
        setRunning(null);
        if (p.code !== 0) {
          setCrash(p);
        } else {
          showToast(t("play.gameFinished"), true);
        }
      }),
    ];
    return () => {
      unlisteners.forEach((p) => p.then((un) => un()));
    };
  }, [logConsole, showToast]);

  // таймер сессии в шапке
  useEffect(() => {
    if (!running) return;
    const iv = window.setInterval(() => setSessionNow(Date.now()), 1000);
    return () => window.clearInterval(iv);
  }, [running]);

  // Drag&drop .jar модов и .mrpack модпаков (Tauri перехватывает dnd сам)
  useEffect(() => {
    const un = getCurrentWebview().onDragDropEvent((e) => {
      const p = e.payload;
      if (p.type === "enter" || p.type === "over") {
        setDragOver(true);
      } else if (p.type === "leave") {
        setDragOver(false);
      } else if (p.type === "drop") {
        setDragOver(false);
        for (const path of p.paths) {
          const lower = path.toLowerCase();
          if (lower.endsWith(".jar")) {
            api
              .installLocalMod(path)
              .then((name) => {
                showToast(`Мод установлен: ${name}`, true);
                setRefreshSignal((n) => n + 1);
              })
              .catch((err) => showToast(String(err), false));
          } else if (lower.endsWith(".mrpack")) {
            api
              .installMrpack(path)
              .then((jobId) => startJob(jobId, "Установка модпака"))
              .catch((err) => showToast(String(err), false));
          } else {
            showToast("Поддерживаются .jar (моды) и .mrpack (модпаки)", false);
          }
        }
      }
    });
    return () => {
      un.then((f) => f()).catch(() => {});
    };
  }, [showToast, startJob]);

  // закрытие меню аккаунтов по клику вне
  useEffect(() => {
    if (!accountMenu) return;
    const close = (e: MouseEvent) => {
      if (accountMenuRef.current && !accountMenuRef.current.contains(e.target as Node)) {
        setAccountMenu(false);
      }
    };
    document.addEventListener("mousedown", close);
    return () => document.removeEventListener("mousedown", close);
  }, [accountMenu]);

  const cancelJob = (jobId: string) => {
    api.cancelJob(jobId).catch(() => {});
  };

  const activeAccount = useMemo(
    () => config?.accounts.find((a) => a.id === config.active_account_id) ?? config?.accounts[0],
    [config],
  );

  // свои обои: подтягиваем base64 при выборе "custom"
  useEffect(() => {
    if (config?.wallpaper === "custom") {
      api
        .getCustomWallpaper()
        .then((d) => setCustomWallpaperData(d))
        .catch(() => setCustomWallpaperData(null));
    }
  }, [config?.wallpaper]);

  if (!config) {
    return (
      <div className="app" style={{ alignItems: "center", justifyContent: "center" }}>
        <div className="row" style={{ gap: 12 }}>
          <LogoMark size={40} />
          <span style={{ fontSize: 16, fontWeight: 700 }}>AstralLauncher</span>
        </div>
      </div>
    );
  }

  // язык применяется до рендера детей — подписи t() всегда актуальны
  setLang(config.language);
  const accent = ACCENTS[config.accent] ?? ACCENTS.violet;
  const accentStyle = {
    "--accent": accent.a,
    "--accent-hover": accent.h,
    "--accent-2": accent.a2,
  } as React.CSSProperties;

  const jobList = Object.values(jobs);

  const WALLPAPERS: Record<string, string> = {
    aurora: "/wallpapers/aurora.jpg",
    nebula: "/wallpapers/nebula.jpg",
    ember: "/wallpapers/ember.jpg",
  };

  const wpSetting = config.wallpaper || "aurora";
  const blur = typeof config.wallpaper_blur === "number" ? config.wallpaper_blur : 18;
  const dim = typeof config.wallpaper_dim === "number" ? config.wallpaper_dim : 40;
  const auroraIntensity = typeof config.aurora_intensity === "number" ? config.aurora_intensity : 55;
  const auroraHue = typeof config.aurora_hue === "number" ? config.aurora_hue : 0;

  let bgImage: string | null = null;
  if (wpSetting === "custom") {
    bgImage = customWallpaperData;
  } else if (wpSetting !== "none" && WALLPAPERS[wpSetting]) {
    bgImage = WALLPAPERS[wpSetting];
  }
  const showBg = bgImage !== null;

  // затемнение поверх обоев, регулируется настройкой (10–80%)
  const d1 = (dim * 0.55) / 100;
  const d2 = dim / 100;

  const sessionSec = running && sessionStart ? Math.max(0, Math.floor((sessionNow - sessionStart) / 1000)) : 0;

  return (
    <div className={`app ${showBg ? "has-bg" : ""}`} style={accentStyle}>
      {showBg ? (
        <div
          className="bg-layer"
          style={{
            backgroundImage: `url(${bgImage})`,
            filter: `blur(${blur}px) brightness(0.92) saturate(1.25)`,
          }}
        />
      ) : null}
      {showBg ? (
        <div
          className="bg-shade"
          style={{
            background: `linear-gradient(180deg, rgba(7,9,16,${d1}) 0%, rgba(7,9,16,${(d1 + d2) / 2}) 55%, rgba(7,9,16,${d2}) 100%)`,
          }}
        />
      ) : null}
      {auroraIntensity > 0 ? (
        <div
          className="aurora-layer"
          style={{
            opacity: auroraIntensity / 100,
            filter: `hue-rotate(${auroraHue}deg)`,
          }}
        />
      ) : null}
      <div className="sidebar">
        <div className="logo">
          <LogoMark size={34} />
        </div>
        {TABS.map((tb) => (
          <button
            key={tb.id}
            className={`nav-btn ${tab === tb.id ? "active" : ""}`}
            title={t(tb.label)}
            onClick={() => setTab(tb.id)}
          >
            {tb.icon}
          </button>
        ))}
        <div className="sidebar-spacer" />
        {activeAccount ? (
          <div className="sidebar-account" title={activeAccount.username}>
            {activeAccount.username.slice(0, 1).toUpperCase()}
          </div>
        ) : null}
      </div>
      <div className="main">
        <div className="topbar">
          <div className="topbar-brand">
            <span className="topbar-name">AstralLauncher</span>
            <span className="badge version-pill">v{launcherVersion || "1.0.1"}</span>
          </div>
          <div className="topbar-right">
            {running ? (
              <div className="running-badge">
                <span className="running-dot" />
                {t("app.running")} · {sessionSec >= 3600 ? `${Math.floor(sessionSec / 3600)}:${String(Math.floor((sessionSec % 3600) / 60)).padStart(2, "0")}:` : ""}{String(Math.floor((sessionSec % 3600) / 60)).padStart(2, "0")}:{String(sessionSec % 60).padStart(2, "0")}
              </div>
            ) : null}
            <div className="account-switch" ref={accountMenuRef}>
              <button
                className="account-switch-btn"
                onClick={() => setAccountMenu((v) => !v)}
                title={t("play.account")}
              >
                <div className="sidebar-account" style={{ width: 28, height: 28, fontSize: 12 }}>
                  {activeAccount ? activeAccount.username.slice(0, 1).toUpperCase() : "?"}
                </div>
                <span className="account-switch-name">
                  {activeAccount ? activeAccount.username : "—"}
                </span>
              </button>
              {accountMenu ? (
                <div className="account-menu">
                  {config.accounts.map((a) => (
                    <button
                      key={a.id}
                      className={`account-menu-item ${a.id === config.active_account_id ? "active" : ""}`}
                      onClick={() => {
                        setAccountMenu(false);
                        api
                          .setActiveAccount(a.id)
                          .then(setConfig)
                          .catch((e) => showToast(String(e), false));
                      }}
                    >
                      <div className="sidebar-account" style={{ width: 24, height: 24, fontSize: 11 }}>
                        {a.username.slice(0, 1).toUpperCase()}
                      </div>
                      <span style={{ flex: 1, textAlign: "left" }}>{a.username}</span>
                      {a.kind === "microsoft" ? <span className="badge loader">MS</span> : null}
                      {a.id === config.active_account_id ? <IconCheck size={14} /> : null}
                    </button>
                  ))}
                  <div className="divider" />
                  <button
                    className="account-menu-item"
                    onClick={() => {
                      setAccountMenu(false);
                      setTab("settings");
                    }}
                  >
                    <IconUser size={16} />
                    <span style={{ flex: 1, textAlign: "left" }}>{t("tab.settings")}</span>
                  </button>
                </div>
              ) : null}
            </div>
          </div>
        </div>
        <div className="content" style={{ paddingBottom: jobList.length > 0 ? 150 : 24 }}>
          {tab === "play" ? (
            <PlayTab
              config={config}
              patchConfig={(p) => patchConfig(p)}
              startJob={startJob}
              showToast={showToast}
              refreshSignal={refreshSignal}
              running={running}
              onLaunched={(info) => {
                setRunning(info);
                setSessionStart(Date.now());
                setSessionNow(Date.now());
                if (config.sounds_enabled) sounds.notify();
              }}
            />
          ) : null}
          {tab === "mods" ? (
            <ModsTab
              config={config}
              startJob={startJob}
              showToast={showToast}
              refreshSignal={refreshSignal}
            />
          ) : null}
          {tab === "settings" ? (
            <SettingsTab
              config={config}
              patchConfig={patchConfig}
              startJob={startJob}
              showToast={showToast}
            />
          ) : null}
          {tab === "console" ? <ConsoleTab logs={logs} onClear={() => setLogs([])} /> : null}
        </div>
      </div>
      {jobList.length > 0 ? (
        <div className="downloads-bar">
          {jobList.map((j) => {
            const pct = j.total_bytes > 0 ? (j.downloaded_bytes / j.total_bytes) * 100 : 0;
            const eta =
              j.speed_bps > 0 && j.remaining_bytes > 0
                ? `${t("download.eta")} ${formatEta(Math.floor(j.remaining_bytes / j.speed_bps))}`
                : "";
            return (
              <div className="download-job" key={j.job_id}>
                <div style={{ width: 210, flexShrink: 0 }}>
                  <div className="label">{j.label}</div>
                  <ProgressBar value={pct} className="progress-in-job" />
                </div>
                <div className="stats">
                  {formatBytes(j.downloaded_bytes)} / {formatBytes(j.total_bytes)} | {t("download.left")}{" "}
                  {formatBytes(j.remaining_bytes)} | {formatSpeed(j.speed_bps)}
                  {eta ? ` | ${eta}` : ""}
                </div>
                <div className="current-file">
                  {j.files_total > 0 ? `${j.files_done}/${j.files_total} ${t("download.files")}` : ""}
                  {j.current_file ? ` — ${j.current_file}` : ""}
                </div>
                <button className="btn sm danger" title={t("download.cancel")} onClick={() => cancelJob(j.job_id)}>
                  <IconCancel size={14} />
                </button>
              </div>
            );
          })}
        </div>
      ) : null}
      {dragOver ? (
        <div className="drop-overlay">
          <div className="drop-box">
            <IconMods size={34} />
            <div style={{ fontWeight: 700, fontSize: 15 }}>{t("mods.dropHint")}</div>
            <div className="hint">{t("mods.dropAny")}</div>
          </div>
        </div>
      ) : null}
      {crash ? (
        <div className="modal-backdrop">
          <div className="modal">
            <h3 style={{ display: "flex", alignItems: "center", gap: 8, color: "var(--error)" }}>
              <IconError size={18} />
              {t("crash.title")}
            </h3>
            <div className="desc" style={{ marginTop: 8 }}>
              {crash.crash ? (
                <>
                  <div style={{ fontWeight: 600, marginBottom: 6 }}>{crash.crash.description}</div>
                  <div>{crash.crash.hint}</div>
                </>
              ) : (
                <>
                  <div style={{ fontWeight: 600, marginBottom: 6 }}>
                    {t("crash.exitCode")}: {crash.code}
                  </div>
                  <div>{t("crash.noCrashFile")}</div>
                </>
              )}
            </div>
            <div className="row" style={{ justifyContent: "flex-end", marginTop: 14 }}>
              {crash.crash ? (
                <button className="btn" onClick={() => api.revealPath(crash.crash!.file).catch((e) => showToast(String(e), false))}>
                  {t("crash.openReport")}
                </button>
              ) : null}
              <button className="btn primary" onClick={() => setCrash(null)}>
                {t("crash.close")}
              </button>
            </div>
          </div>
        </div>
      ) : null}
      <div className="toast">
        {toasts.map((tm) => (
          <div key={tm.id} className={`toast-item ${tm.ok ? "ok" : "fail"}`}>
            <span style={{ color: tm.ok ? "var(--success)" : "var(--error)", flexShrink: 0, paddingTop: 1 }}>
              {tm.ok ? <IconCheck size={15} /> : <IconError size={15} />}
            </span>
            <span style={{ wordBreak: "break-word" }}>{tm.msg}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
