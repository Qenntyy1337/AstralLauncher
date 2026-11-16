import { useEffect, useMemo, useRef, useState } from "react";
import { api, formatDate, formatBytes } from "../api";
import { t } from "../i18n";
import type { Config, ManifestVersion, InstalledVersion, SizeBreakdown, LaunchInfo, PlayStats } from "../types";
import {
  IconClock,
  IconDownload,
  IconPlay,
  IconStop,
  IconCheck,
  IconUser,
  IconCPU,
  IconRefresh,
  IconStats,
} from "../icons";
import { EmptyState, Spinner } from "./common";

interface PlayTabProps {
  config: Config;
  patchConfig: (patch: Partial<Config>) => void;
  startJob: (jobId: string, label: string) => void;
  showToast: (msg: string, ok?: boolean) => void;
  refreshSignal: number;
  running: LaunchInfo | null;
  onLaunched: (info: LaunchInfo) => void;
}

const LOADERS: { id: string; label: string }[] = [
  { id: "vanilla", label: "Vanilla" },
  { id: "fabric", label: "Fabric" },
  { id: "forge", label: "Forge" },
];

function versionGroup(kind: string): string {
  if (kind === "release") return "release";
  if (kind === "snapshot") return "snapshot";
  return "old";
}

export function formatDuration(sec: number): string {
  if (sec >= 3600) {
    return `${Math.floor(sec / 3600)} ${t("play.hours")} ${Math.floor((sec % 3600) / 60)} ${t("play.minutes")}`;
  }
  if (sec >= 60) {
    return `${Math.floor(sec / 60)} ${t("play.minutes")}`;
  }
  return `${sec} ${t("play.minutes")}`;
}

export default function PlayTab({
  config,
  patchConfig,
  startJob,
  showToast,
  refreshSignal,
  running,
  onLaunched,
}: PlayTabProps) {
  const [versions, setVersions] = useState<ManifestVersion[] | null>(null);
  const [installed, setInstalled] = useState<InstalledVersion[]>([]);
  const [sizes, setSizes] = useState<Record<string, number>>({});
  const [filter, setFilter] = useState<string>("release");
  const [query, setQuery] = useState("");
  // выбранный загрузчик хранится в конфиге и не сбрасывается при смене вкладки
  const loader = config.play_loader || "vanilla";
  const setLoader = (v: string) => patchConfig({ play_loader: v });
  const [selected, setSelected] = useState<string>(config.active_version ?? "");
  const [launching, setLaunching] = useState(false);
  const [javaPrompt, setJavaPrompt] = useState<number | null>(null);
  const [stats, setStats] = useState<PlayStats | null>(null);
  const fetchGen = useRef(0);

  const reloadInstalled = () => {
    api.listInstalled().then(setInstalled).catch(() => {});
  };

  const reloadStats = () => {
    api.getPlayStats().then(setStats).catch(() => {});
  };

  useEffect(() => {
    api.listVersions().then((v) => setVersions(v)).catch((e) => showToast(String(e), false));
    reloadInstalled();
    reloadStats();
  }, []);

  useEffect(() => {
    reloadInstalled();
    reloadStats();
  }, [refreshSignal]);

  // статистика обновляется после завершения сессии
  useEffect(() => {
    if (!running) {
      reloadStats();
    }
  }, [running]);

  const filtered = useMemo(() => {
    if (!versions) return [];
    const q = query.trim().toLowerCase();
    return versions.filter(
      (v) =>
        (filter === "all" || versionGroup(v.kind) === filter) &&
        (q === "" || v.id.toLowerCase().includes(q)),
    );
  }, [versions, filter, query]);

  useEffect(() => {
    let cancelled = false;
    const gen = ++fetchGen.current;
    const targets = filtered.slice(0, 60).map((v) => v.id).filter((id) => !(id in sizes));
    // Небольшие параллельные партии, чтобы размеры появлялись быстро,
    // но без очереди из 60 запросов к Mojang
    const BATCH = 8;
    const runBatch = (start: number) => {
      if (cancelled || gen !== fetchGen.current) return;
      const batch = targets.slice(start, start + BATCH);
      if (batch.length === 0) return;
      Promise.all(
        batch.map((id) =>
          api
            .versionSize(id)
            .then((s: SizeBreakdown) => {
              if (gen === fetchGen.current && !cancelled) {
                setSizes((prev) => ({ ...prev, [id]: s.total_bytes }));
              }
            })
            .catch(() => {}),
        ),
      ).then(() => runBatch(start + BATCH));
    };
    runBatch(0);
    return () => {
      cancelled = true;
    };
  }, [filtered, sizes]);

  const install = (id: string) => {
    api
      .installVersion(id, loader === "vanilla" ? null : loader, null)
      .then((jobId) => startJob(jobId, `Установка ${id}${loader === "vanilla" ? "" : ` (${loader})`}`))
      .catch((e) => showToast(String(e), false));
  };

  useEffect(() => {
    // бэкенд делает только что установленную версию активной — подхватываем
    if (config.active_version) {
      setSelected(config.active_version);
    }
  }, [config.active_version]);

  const launch = () => {
    const versionId = selected || config.active_version || "";
    if (!versionId) {
      showToast(t("play.selectVersion"), false);
      return;
    }
    setLaunching(true);
    setJavaPrompt(null);
    api
      .launchGame(versionId)
      .then((info) => {
        onLaunched(info);
        patchConfig({ active_version: versionId });
        showToast(`${t("play.gameStarted")}: ${info.version}`, true);
      })
      .catch((e) => {
        const msg = String(e);
        if (msg.startsWith("JAVA_MISSING:")) {
          const major = parseInt(msg.split(":")[1], 10) || 17;
          setJavaPrompt(major);
          showToast(`${t("play.javaMissing")} ${major}${t("play.javaMissingHint")}`, false);
        } else {
          showToast(msg, false);
        }
      })
      .finally(() => setLaunching(false));
  };

  const installJava = (major: number) => {
    api
      .installJava(major)
      .then((jobId) => startJob(jobId, `Установка Java ${major} (Temurin JRE)`))
      .catch((e) => showToast(String(e), false));
  };

  const stopGame = () => {
    if (!running) return;
    api
      .stopGame(running.pid)
      .catch((e) => showToast(String(e), false));
  };

  const account = config.accounts.find((a) => a.id === config.active_account_id) ?? config.accounts[0];
  const selectedInstalled = installed.find((i) => i.id === (selected || config.active_version));

  const topVersions = useMemo(() => {
    if (!stats) return [];
    return Object.entries(stats.per_version_sec)
      .sort((a, b) => b[1] - a[1])
      .slice(0, 3);
  }, [stats]);

  const history = useMemo(() => (stats ? stats.history.slice(0, 8) : []), [stats]);

  return (
    <div>
      <div className="page-title">{t("play.title")}</div>
      <div className="page-subtitle">{t("play.subtitle")}</div>
      <div className="play-layout">
        <div className="card">
          <div className="card-title">
            <IconDownload size={16} />
            {t("play.catalog")}
          </div>
          <div className="filter-bar">
            <div className="row" style={{ gap: 6 }}>
              {[
                { id: "release", label: t("play.release") },
                { id: "snapshot", label: t("play.snapshot") },
                { id: "old", label: t("play.old") },
                { id: "all", label: t("play.all") },
              ].map((f) => (
                <button
                  key={f.id}
                  className={`chip ${filter === f.id ? "active" : ""}`}
                  onClick={() => setFilter(f.id)}
                >
                  {f.label}
                </button>
              ))}
            </div>
            <input
              className="input"
              style={{ flex: 1, minWidth: 140 }}
              placeholder={t("play.searchVersion")}
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
            <select
              className="select"
              value={loader}
              onChange={(e) => setLoader(e.target.value)}
              title="Мод-лоадер для установки"
            >
              {LOADERS.map((l) => (
                <option key={l.id} value={l.id}>
                  {l.label}
                </option>
              ))}
            </select>
          </div>
          {!versions ? (
            <div className="row" style={{ justifyContent: "center", padding: 30 }}>
              <Spinner />
            </div>
          ) : filtered.length === 0 ? (
            <EmptyState icon={<IconDownload size={28} />} title={t("play.nothingInstalled")} />
          ) : (
            <div className="list">
              {filtered.slice(0, 60).map((v) => {
                const size = sizes[v.id];
                const isInstalled = installed.some((i) => i.id === v.id || i.id.includes(v.id));
                const isSnapshot = versionGroup(v.kind) === "snapshot";
                const isOld = versionGroup(v.kind) === "old";
                const forgeBlocked = loader === "forge" && (isSnapshot || v.id.includes("snapshot"));
                return (
                  <div className="list-item version-row" key={v.id}>
                    <div className="grow">
                      <div className="name">{v.id}</div>
                      <div className="meta row" style={{ gap: 10, marginTop: 2 }}>
                        <span className="row" style={{ gap: 4 }}>
                          <IconClock size={12} />
                          {formatDate(v.release_time) || "—"}
                        </span>
                        <span>{size !== undefined ? formatBytes(size) : "…"}</span>
                      </div>
                    </div>
                    {isSnapshot ? <span className="badge snapshot">{t("play.snapshotBadge")}</span> : null}
                    {isOld ? <span className="badge old">{t("play.oldBadge")}</span> : null}
                    {!isSnapshot && !isOld ? <span className="badge release">{t("play.releaseBadge")}</span> : null}
                    {isInstalled ? (
                      <span className="badge installed">
                        <IconCheck size={11} />
                        {t("play.installed")}
                      </span>
                    ) : null}
                    <button
                      className="btn sm"
                      onClick={() => install(v.id)}
                      disabled={forgeBlocked}
                      title={
                        forgeBlocked
                          ? "Forge доступен только для релизов — выберите Vanilla или Fabric"
                          : loader === "vanilla"
                            ? "Установить версию"
                            : `Установить ${loader}`
                      }
                    >
                      <IconDownload size={14} />
                      {t("play.install")}
                    </button>
                  </div>
                );
              })}
            </div>
          )}
          <div className="hint">{t("play.catalogHint")}</div>
        </div>
        <div className="col">
          <div className="card">
            <div className="card-title">
              <IconUser size={16} />
              {t("play.account")}
            </div>
            {account ? (
              <div className="row" style={{ justifyContent: "space-between" }}>
                <div className="row" style={{ gap: 10 }}>
                  <div className="sidebar-account">{account.username.slice(0, 1).toUpperCase()}</div>
                  <div>
                    <div style={{ fontWeight: 600 }}>{account.username}</div>
                    <div className="meta" style={{ color: "var(--text-faint)", fontSize: 12 }}>
                      {account.kind === "microsoft" ? "Microsoft" : t("play.offline")}
                    </div>
                  </div>
                </div>
                <span className="badge">{t("play.changeInSettings")}</span>
              </div>
            ) : (
              <div className="hint">{t("play.noAccount")}</div>
            )}
          </div>
          <div className="card">
            <div className="card-title">
              <IconCPU size={16} />
              {t("play.installedVersions")}
              <span style={{ marginLeft: "auto" }}>
                <a
                  className="link"
                  href="#refresh"
                  onClick={(e) => {
                    e.preventDefault();
                    reloadInstalled();
                  }}
                >
                  <IconRefresh size={14} />
                </a>
              </span>
            </div>
            {installed.length === 0 ? (
              <div className="hint">{t("play.nothingInstalled")}</div>
            ) : (
              <div className="installed-col">
                {installed.map((iv) => (
                  <div
                    key={iv.id}
                    className={`installed-row ${
                      selected === iv.id || (!selected && config.active_version === iv.id) ? "selected" : ""
                    }`}
                    onClick={() => {
                      setSelected(iv.id);
                      api.setActiveVersion(iv.id).catch(() => {});
                    }}
                  >
                    <div className="grow" style={{ flex: 1, minWidth: 0 }}>
                      <div
                        style={{
                          fontWeight: 600,
                          fontSize: 13,
                          whiteSpace: "nowrap",
                          overflow: "hidden",
                          textOverflow: "ellipsis",
                        }}
                      >
                        {iv.id}
                      </div>
                      <div className="meta" style={{ color: "var(--text-faint)", fontSize: 11.5 }}>
                        {iv.has_jar ? t("play.readyToLaunch") : t("play.noJar")}
                      </div>
                    </div>
                    {iv.loader !== "vanilla" ? <span className="badge loader">{iv.loader}</span> : null}
                  </div>
                ))}
              </div>
            )}
            <div style={{ marginTop: 14 }} className="row">
              <button
                className="btn primary play"
                onClick={launch}
                disabled={launching || (!selectedInstalled && !selected)}
                style={{ flex: 1 }}
              >
                {launching ? <Spinner /> : <IconPlay size={18} />}
                {launching ? t("play.launching") : running ? t("play.running") : t("play.launch")}
              </button>
              {running ? (
                <button className="btn danger" onClick={stopGame} title={t("play.stop")}>
                  <IconStop size={16} />
                </button>
              ) : null}
            </div>
            {running ? (
              <div className="hint">
                PID {running.pid} | Java {running.java.split("\\").pop()} | {running.memory_mb} МБ
              </div>
            ) : null}
            {javaPrompt !== null ? (
              <button className="btn primary" style={{ marginTop: 10, width: "100%" }} onClick={() => installJava(javaPrompt)}>
                <IconDownload size={16} />
                {t("play.javaPrompt")} {javaPrompt}
              </button>
            ) : null}
          </div>

          <div className="card">
            <div className="card-title">
              <IconStats size={16} />
              {t("play.stats")}
            </div>
            {stats && stats.total_sec > 0 ? (
              <>
                <div className="stat-row">
                  <span className="stat-value">{formatDuration(stats.total_sec)}</span>
                  <span className="stat-label">{t("play.total")}</span>
                </div>
                {topVersions.length > 0 ? (
                  <div className="installed-col" style={{ marginTop: 10 }}>
                    {topVersions.map(([v, sec]) => (
                      <div key={v} className="installed-row" style={{ cursor: "default" }}>
                        <div style={{ flex: 1, minWidth: 0 }}>
                          <div style={{ fontSize: 12.5, fontWeight: 500 }}>{v}</div>
                        </div>
                        <span className="badge">{formatDuration(sec)}</span>
                      </div>
                    ))}
                  </div>
                ) : null}
              </>
            ) : (
              <div className="hint">{t("play.noStats")}</div>
            )}
          </div>

          <div className="card">
            <div className="card-title">
              <IconClock size={16} />
              {t("play.history")}
            </div>
            {history.length === 0 ? (
              <div className="hint">{t("play.noHistory")}</div>
            ) : (
              <div className="installed-col">
                {history.map((h, i) => (
                  <div key={i} className="installed-row" style={{ cursor: "default" }}>
                    <div style={{ flex: 1, minWidth: 0 }}>
                      <div style={{ fontSize: 12.5, fontWeight: 500, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                        {h.version}
                      </div>
                      <div className="meta" style={{ color: "var(--text-faint)", fontSize: 11.5 }}>
                        {new Date(h.ts_ms).toLocaleString("ru-RU", {
                          day: "2-digit",
                          month: "2-digit",
                          hour: "2-digit",
                          minute: "2-digit",
                        })}
                        {h.account ? ` · ${h.account}` : ""}
                      </div>
                    </div>
                    <span className="badge">{h.duration_sec > 0 ? formatDuration(h.duration_sec) : "—"}</span>
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
