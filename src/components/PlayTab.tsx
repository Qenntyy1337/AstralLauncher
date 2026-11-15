import { useEffect, useMemo, useRef, useState } from "react";
import { api, formatDate, formatBytes } from "../api";
import type { Config, ManifestVersion, InstalledVersion, SizeBreakdown, LaunchInfo } from "../types";
import {
  IconClock,
  IconDownload,
  IconPlay,
  IconStop,
  IconCheck,
  IconUser,
  IconCPU,
  IconRefresh,
} from "../icons";
import { EmptyState, Spinner } from "./common";

interface PlayTabProps {
  config: Config;
  patchConfig: (patch: Partial<Config>) => void;
  startJob: (jobId: string, label: string) => void;
  showToast: (msg: string, ok?: boolean) => void;
  refreshSignal: number;
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

export default function PlayTab({ config, patchConfig, startJob, showToast, refreshSignal }: PlayTabProps) {
  const [versions, setVersions] = useState<ManifestVersion[] | null>(null);
  const [installed, setInstalled] = useState<InstalledVersion[]>([]);
  const [sizes, setSizes] = useState<Record<string, number>>({});
  const [filter, setFilter] = useState<string>("release");
  const [query, setQuery] = useState("");
  const [loader, setLoader] = useState("vanilla");
  const [selected, setSelected] = useState<string>(config.active_version ?? "");
  const [launching, setLaunching] = useState(false);
  const [running, setRunning] = useState<LaunchInfo | null>(null);
  const [javaPrompt, setJavaPrompt] = useState<number | null>(null);
  const fetchGen = useRef(0);

  const reloadInstalled = () => {
    api.listInstalled().then(setInstalled).catch(() => {});
  };

  useEffect(() => {
    api.listVersions().then((v) => setVersions(v)).catch((e) => showToast(String(e), false));
    reloadInstalled();
  }, []);

  useEffect(() => {
    reloadInstalled();
  }, [refreshSignal]);

  useEffect(() => {
    let cancelled = false;
    const handler = (payload: { pid: number; code: number }) => {
      setRunning(null);
      showToast(`Игра завершена (код ${payload.code})`, payload.code === 0);
    };
    import("@tauri-apps/api/event").then(({ listen }) => {
      listen<{ pid: number; code: number }>("game-exit", (e) => handler(e.payload)).then((un) => {
        if (cancelled) un();
      });
    });
    return () => {
      cancelled = true;
    };
  }, [showToast]);

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
    const gen = ++fetchGen.current;
    const targets = filtered.slice(0, 60).map((v) => v.id).filter((id) => !(id in sizes));
    let cancelled = false;
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

  const launch = () => {
    const versionId = selected || config.active_version || "";
    if (!versionId) {
      showToast("Выберите установленную версию", false);
      return;
    }
    setLaunching(true);
    setJavaPrompt(null);
    api
      .launchGame(versionId)
      .then((info) => {
        setRunning(info);
        patchConfig({ active_version: versionId });
        showToast(`Игра запущена: ${info.version}`, true);
      })
      .catch((e) => {
        const msg = String(e);
        if (msg.startsWith("JAVA_MISSING:")) {
          const major = parseInt(msg.split(":")[1], 10) || 17;
          setJavaPrompt(major);
          showToast(`Не найдена Java ${major}. Установите её одним кликом ниже.`, false);
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

  return (
    <div>
      <div className="page-title">Игра</div>
      <div className="page-subtitle">Версии Minecraft, мод-лоадеры и запуск</div>
      <div className="play-layout">
        <div className="card">
          <div className="card-title">
            <IconDownload size={16} />
            Каталог версий
          </div>
          <div className="filter-bar">
            <div className="row" style={{ gap: 6 }}>
              {[
                { id: "release", label: "Релизы" },
                { id: "snapshot", label: "Снапшоты" },
                { id: "old", label: "Старые" },
                { id: "all", label: "Все" },
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
              placeholder="Поиск версии, например 1.20.1"
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
            <EmptyState icon={<IconDownload size={28} />} title="Версии не найдены" />
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
                    {isSnapshot ? <span className="badge snapshot">снапшот</span> : null}
                    {isOld ? <span className="badge old">старая</span> : null}
                    {isInstalled ? (
                      <span className="badge release">
                        <IconCheck size={11} />
                        есть
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
                      Установить
                    </button>
                  </div>
                );
              })}
            </div>
          )}
          <div className="hint">
            Размер — клиентский JAR, библиотеки и ассеты, считается до установки. Дата — релиз
            версии. Загрузчик можно сменить перед установкой: Fabric или Forge ставятся поверх
            выбранной версии. Forge доступен только для релизов, снапшоты — Vanilla и Fabric.
          </div>
        </div>
        <div className="col">
          <div className="card">
            <div className="card-title">
              <IconUser size={16} />
              Аккаунт
            </div>
            {account ? (
              <div className="row" style={{ justifyContent: "space-between" }}>
                <div className="row" style={{ gap: 10 }}>
                  <div className="sidebar-account">{account.username.slice(0, 1).toUpperCase()}</div>
                  <div>
                    <div style={{ fontWeight: 600 }}>{account.username}</div>
                    <div className="meta" style={{ color: "var(--text-faint)", fontSize: 12 }}>
                      {account.kind === "microsoft" ? "Microsoft" : "Офлайн"}
                    </div>
                  </div>
                </div>
                <span className="badge">сменить в настройках</span>
              </div>
            ) : (
              <div className="hint">Аккаунт не выбран. Добавьте его во вкладке Настройки.</div>
            )}
          </div>
          <div className="card">
            <div className="card-title">
              <IconCPU size={16} />
              Установленные версии
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
              <div className="hint">Пока ничего не установлено. Выберите версию в каталоге слева.</div>
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
                        {iv.has_jar ? "готова к запуску" : "нет jar"}
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
                {launching ? "Запуск..." : running ? "Игра запущена" : "Играть"}
              </button>
              {running ? (
                <button className="btn danger" onClick={stopGame} title="Завершить процесс игры">
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
                Установить Java {javaPrompt}
              </button>
            ) : null}
          </div>
        </div>
      </div>
    </div>
  );
}
