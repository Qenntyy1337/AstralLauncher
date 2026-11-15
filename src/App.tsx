import { useCallback, useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, formatBytes, formatSpeed } from "./api";
import type { Config, ConsoleEntry, ProgressPayload, TaskFinished } from "./types";
import {
  IconPlay,
  IconMods,
  IconSettings,
  IconConsole,
  IconCancel,
  LogoMark,
  IconCheck,
  IconError,
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
  { id: "play", label: "Игра", icon: <IconPlay size={21} /> },
  { id: "mods", label: "Модификации", icon: <IconMods size={21} /> },
  { id: "settings", label: "Настройки", icon: <IconSettings size={21} /> },
  { id: "console", label: "Консоль", icon: <IconConsole size={21} /> },
];

export default function App() {
  const [config, setConfig] = useState<Config | null>(null);
  const [tab, setTab] = useState<TabId>("play");
  const [logs, setLogs] = useState<ConsoleEntry[]>([]);
  const [jobs, setJobs] = useState<Record<string, ProgressPayload>>({});
  const [toasts, setToasts] = useState<Toast[]>([]);
  const [refreshSignal, setRefreshSignal] = useState(0);

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
        logConsole({
          ts: new Date().toLocaleTimeString("ru-RU", { hour12: false }),
          level: t.ok ? "info" : "error",
          source: "task",
          message: t.message,
        });
        if (t.ok && (t.kind === "install-version" || t.kind === "install-content" || t.kind === "install-java")) {
          setRefreshSignal((n) => n + 1);
        }
      }),
      listen<ConsoleEntry>("console-log", (e) => logConsole(e.payload)),
    ];
    return () => {
      unlisteners.forEach((p) => p.then((un) => un()));
    };
  }, [logConsole, showToast]);

  const cancelJob = (jobId: string) => {
    api.cancelJob(jobId).catch(() => {});
  };

  const activeAccount = useMemo(
    () => config?.accounts.find((a) => a.id === config.active_account_id) ?? config?.accounts[0],
    [config],
  );

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

  const jobList = Object.values(jobs);

  const WALLPAPERS: Record<string, string> = {
    aurora: "/wallpapers/aurora.jpg",
    nebula: "/wallpapers/nebula.jpg",
    ember: "/wallpapers/ember.jpg",
  };

  const wallpaper = config.wallpaper && WALLPAPERS[config.wallpaper] ? config.wallpaper : "aurora";

  return (
    <div className={`app ${wallpaper !== "none" ? "has-bg" : ""}`}>
      {wallpaper !== "none" ? (
        <div
          className="bg-layer"
          style={{ backgroundImage: `url(${WALLPAPERS[wallpaper]})` }}
        />
      ) : null}
      <div className="sidebar">
        <div className="logo">
          <LogoMark size={34} />
        </div>
        {TABS.map((t) => (
          <button
            key={t.id}
            className={`nav-btn ${tab === t.id ? "active" : ""}`}
            title={t.label}
            onClick={() => setTab(t.id)}
          >
            {t.icon}
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
        <div className="content" style={{ paddingBottom: jobList.length > 0 ? 150 : 24 }}>
          {tab === "play" ? (
            <PlayTab
              config={config}
              patchConfig={(p) => patchConfig(p)}
              startJob={startJob}
              showToast={showToast}
              refreshSignal={refreshSignal}
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
            return (
              <div className="download-job" key={j.job_id}>
                <div style={{ width: 210, flexShrink: 0 }}>
                  <div className="label">{j.label}</div>
                  <ProgressBar value={pct} className="progress-in-job" />
                </div>
                <div className="stats">
                  {formatBytes(j.downloaded_bytes)} / {formatBytes(j.total_bytes)} | осталось{" "}
                  {formatBytes(j.remaining_bytes)} | {formatSpeed(j.speed_bps)}
                </div>
                <div className="current-file">
                  {j.files_total > 0 ? `${j.files_done}/${j.files_total} файлов` : ""}
                  {j.current_file ? ` — ${j.current_file}` : ""}
                </div>
                <button className="btn sm danger" title="Отменить" onClick={() => cancelJob(j.job_id)}>
                  <IconCancel size={14} />
                </button>
              </div>
            );
          })}
        </div>
      ) : null}
      <div className="toast">
        {toasts.map((t) => (
          <div key={t.id} className={`toast-item ${t.ok ? "ok" : "fail"}`}>
            <span style={{ color: t.ok ? "var(--success)" : "var(--error)", flexShrink: 0, paddingTop: 1 }}>
              {t.ok ? <IconCheck size={15} /> : <IconError size={15} />}
            </span>
            <span style={{ wordBreak: "break-word" }}>{t.msg}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
