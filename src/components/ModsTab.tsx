import { useEffect, useState } from "react";
import { api, formatBytes, formatNumber, formatDate } from "../api";
import type { SearchHit, Project, ModVersion, FileInfo, Config } from "../types";
import {
  IconSearch,
  IconDownload,
  IconBox,
  IconImage,
  IconGlobe,
  IconTrash,
  IconCheck,
  IconUser,
} from "../icons";
import { EmptyState, Spinner } from "./common";

interface ModsTabProps {
  config: Config;
  startJob: (jobId: string, label: string) => void;
  showToast: (msg: string, ok?: boolean) => void;
  refreshSignal: number;
}

const TYPES = [
  { id: "mod", label: "Моды" },
  { id: "resourcepack", label: "Ресурспаки" },
  { id: "shader", label: "Шейдеры" },
  { id: "world", label: "Карты" },
];

const LOADERS = ["fabric", "forge", "neoforge", "quilt"];

const TYPE_DIRS: Record<string, string> = {
  mod: "mods",
  resourcepack: "resourcepacks",
  shader: "shaderpacks",
  world: "saves",
};

export default function ModsTab({ config, startJob, showToast, refreshSignal }: ModsTabProps) {
  const [type, setType] = useState("mod");
  const [query, setQuery] = useState("");
  const [mc, setMc] = useState("");
  const [loader, setLoader] = useState("fabric");
  const [hits, setHits] = useState<SearchHit[]>([]);
  const [total, setTotal] = useState(0);
  const [offset, setOffset] = useState(0);
  const [loading, setLoading] = useState(false);
  const [releases, setReleases] = useState<string[]>([]);
  const [details, setDetails] = useState<Project | null>(null);
  const [versionsList, setVersionsList] = useState<ModVersion[] | null>(null);
  const [pickedVersion, setPickedVersion] = useState("");
  const [installedFiles, setInstalledFiles] = useState<FileInfo[]>([]);
  const [busyCard, setBusyCard] = useState<string | null>(null);

  useEffect(() => {
    api
      .listVersions()
      .then((v) => setReleases(v.filter((x) => x.kind === "release").map((x) => x.id)))
      .catch(() => {});
  }, []);

  useEffect(() => {
    const stored = config.active_version;
    if (stored && releases.includes(stored)) {
      setMc((prev) => prev || stored);
    }
  }, [releases, config.active_version]);

  const reloadFiles = () => {
    api.listContentFiles(type).then(setInstalledFiles).catch(() => {});
  };

  useEffect(() => {
    reloadFiles();
  }, [type, refreshSignal]);

  const search = (newOffset: number) => {
    setLoading(true);
    api
      .modrinthSearch(query, type, mc || null, type === "mod" ? loader : null, newOffset)
      .then((r) => {
        setTotal(r.total_hits);
        setHits((prev) => (newOffset === 0 ? r.hits : [...prev, ...r.hits]));
        setOffset(newOffset);
      })
      .catch((e) => showToast(String(e), false))
      .finally(() => setLoading(false));
  };

  useEffect(() => {
    search(0);
  }, [type]);

  const openDetails = (hit: SearchHit) => {
    setDetails(null);
    setVersionsList(null);
    setPickedVersion("");
    api
      .modrinthProject(hit.project_id)
      .then((p) => {
        setDetails(p);
        return api.modrinthVersions(p.slug, mc || null, type === "mod" ? loader : null);
      })
      .then((v) => {
        setVersionsList(v);
        const compatible = v.find((x) => mc === "" || x.game_versions.includes(mc));
        if (compatible) setPickedVersion(compatible.id);
      })
      .catch((e) => {
        showToast(String(e), false);
        setDetails({ ...hit, body: "", downloads: hit.downloads, loaders: [], game_versions: [], license: null } as Project);
      });
  };

  const installSelected = () => {
    if (!details) return;
    if (!mc) {
      showToast("Выберите версию игры в фильтре сверху", false);
      return;
    }
    setBusyCard(details.project_id);
    api
      .modrinthInstall(
        details.project_id,
        details.title,
        details.project_type || type,
        pickedVersion || null,
        mc,
        type === "mod" ? loader : "",
      )
      .then((jobId) => startJob(jobId, `Установка: ${details.title}`))
      .catch((e) => showToast(String(e), false))
      .finally(() => setBusyCard(null));
  };

  const quickInstall = (hit: SearchHit) => {
    if (!mc) {
      showToast("Выберите версию игры в фильтре сверху", false);
      return;
    }
    setBusyCard(hit.project_id);
    api
      .modrinthInstall(hit.project_id, hit.title, hit.project_type || type, null, mc, type === "mod" ? loader : "")
      .then((jobId) => startJob(jobId, `Установка: ${hit.title}`))
      .catch((e) => showToast(String(e), false))
      .finally(() => setBusyCard(null));
  };

  const deleteFile = (name: string) => {
    api
      .deleteContentFile(type, name)
      .then(() => {
        showToast(`Удалено: ${name}`, true);
        reloadFiles();
      })
      .catch((e) => showToast(String(e), false));
  };

  return (
    <div>
      <div className="page-title">Модификации</div>
      <div className="page-subtitle">Каталог Modrinth: моды, ресурспаки, шейдеры и карты</div>
      <div className="mods-layout">
        <div>
          <div className="card" style={{ padding: 14 }}>
            <div className="row wrap">
              <div className="row" style={{ gap: 6 }}>
                {TYPES.map((t) => (
                  <button
                    key={t.id}
                    className={`chip ${type === t.id ? "active" : ""}`}
                    onClick={() => setType(t.id)}
                  >
                    {t.label}
                  </button>
                ))}
              </div>
              <div className="search-input-wrap">
                <IconSearch size={15} />
                <input
                  className="input"
                  placeholder="Поиск по названию"
                  value={query}
                  onChange={(e) => setQuery(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") search(0);
                  }}
                />
              </div>
              <select className="select" value={mc} onChange={(e) => setMc(e.target.value)}>
                <option value="">Версия игры...</option>
                {releases.map((r) => (
                  <option key={r} value={r}>
                    {r}
                  </option>
                ))}
              </select>
              {type === "mod" ? (
                <select className="select" value={loader} onChange={(e) => setLoader(e.target.value)}>
                  {LOADERS.map((l) => (
                    <option key={l} value={l}>
                      {l}
                    </option>
                  ))}
                </select>
              ) : null}
              <button className="btn primary" onClick={() => search(0)}>
                <IconSearch size={14} />
                Найти
              </button>
            </div>
          </div>
          {loading && hits.length === 0 ? (
            <div className="row" style={{ justifyContent: "center", padding: 40 }}>
              <Spinner />
            </div>
          ) : hits.length === 0 ? (
            <EmptyState icon={<IconBox size={28} />} title="Ничего не найдено" hint="Измените запрос или фильтры" />
          ) : (
            <>
              <div className="mods-grid">
                {hits.map((h) => (
                  <div className="mod-card" key={h.project_id} onClick={() => openDetails(h)}>
                    {h.icon_url ? (
                      <img className="mod-icon" src={h.icon_url} alt="" loading="lazy" />
                    ) : (
                      <div className="mod-icon-fallback">
                        <IconImage size={26} />
                      </div>
                    )}
                    <div className="mod-card-body">
                      <div className="mod-title">{h.title}</div>
                      <div className="mod-desc">{h.description}</div>
                      <div className="mod-meta">
                        <span className="row" style={{ gap: 4 }}>
                          <IconDownload size={12} />
                          {formatNumber(h.downloads)}
                        </span>
                        <span className="row" style={{ gap: 4 }}>
                          <IconUser size={12} />
                          {h.author}
                        </span>
                      </div>
                    </div>
                    <button
                      className="btn sm primary"
                      style={{ margin: "0 14px 14px" }}
                      onClick={(e) => {
                        e.stopPropagation();
                        quickInstall(h);
                      }}
                      disabled={busyCard === h.project_id}
                    >
                      <IconDownload size={13} />
                      Скачать
                    </button>
                  </div>
                ))}
              </div>
              {hits.length < total ? (
                <div className="row" style={{ justifyContent: "center", marginTop: 14 }}>
                  <button className="btn" onClick={() => search(offset + 20)} disabled={loading}>
                    {loading ? <Spinner /> : null}
                    Показать ещё ({formatNumber(total - hits.length)})
                  </button>
                </div>
              ) : null}
            </>
          )}
        </div>
        <div className="card">
          <div className="card-title">
            <IconCheck size={16} />
            Установлено: {TYPES.find((t) => t.id === type)?.label}
          </div>
          {installedFiles.length === 0 ? (
            <div className="hint">Папка пуста. Скачайте что-нибудь из каталога.</div>
          ) : (
            <div className="installed-col">
              {installedFiles.map((f) => (
                <div key={f.name} className="installed-row" style={{ cursor: "default" }}>
                  <div style={{ flex: 1, minWidth: 0 }}>
                    <div
                      style={{
                        fontSize: 12.5,
                        fontWeight: 500,
                        whiteSpace: "nowrap",
                        overflow: "hidden",
                        textOverflow: "ellipsis",
                      }}
                    >
                      {f.name}
                    </div>
                    <div style={{ color: "var(--text-faint)", fontSize: 11.5 }}>{formatBytes(f.size_bytes)}</div>
                  </div>
                  <button className="btn sm danger" title="Удалить" onClick={() => deleteFile(f.name)}>
                    <IconTrash size={13} />
                  </button>
                </div>
              ))}
            </div>
          )}
          <div className="hint">
            Файлы сохраняются в {config.root_dir}\minecraft\{TYPE_DIRS[type]}
          </div>
        </div>
      </div>
      {details ? (
        <div className="modal-backdrop" onClick={() => setDetails(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <div className="row" style={{ alignItems: "flex-start", gap: 14 }}>
              {details.icon_url ? (
                <img
                  src={details.icon_url}
                  alt=""
                  style={{ width: 64, height: 64, borderRadius: 10, objectFit: "cover" }}
                />
              ) : (
                <div
                  className="mod-icon-fallback"
                  style={{ width: 64, height: 64, borderRadius: 10, flexShrink: 0 }}
                >
                  <IconImage size={24} />
                </div>
              )}
              <div style={{ flex: 1, minWidth: 0 }}>
                <h3>{details.title}</h3>
                <div className="desc">{details.description}</div>
                <div className="row wrap" style={{ marginTop: 8, gap: 6 }}>
                  <span className="badge">
                    <IconDownload size={11} />
                    {formatNumber(details.downloads)}
                  </span>
                  {details.license?.name || details.license?.id ? (
                    <span className="badge">
                      <IconGlobe size={11} />
                      {details.license?.name || details.license?.id}
                    </span>
                  ) : null}
                  {(details.loaders || []).map((l) => (
                    <span key={l} className="badge loader">
                      {l}
                    </span>
                  ))}
                </div>
              </div>
            </div>
            <div className="divider" />
            {details.body ? (
              <div
                className="desc"
                style={{
                  maxHeight: 180,
                  overflowY: "auto",
                  whiteSpace: "pre-wrap",
                  fontSize: 12.5,
                  userSelect: "text",
                }}
              >
                {details.body.slice(0, 2000)}
              </div>
            ) : null}
            <div className="divider" />
            <div>
              <span className="label">Версия релиза</span>
              {versionsList === null ? (
                <div className="row" style={{ padding: 8 }}>
                  <Spinner />
                </div>
              ) : versionsList.length === 0 ? (
                <div className="hint">Нет сборок для выбранных фильтров (игра {mc || "любая"})</div>
              ) : (
                <select
                  className="select"
                  style={{ width: "100%" }}
                  value={pickedVersion}
                  onChange={(e) => setPickedVersion(e.target.value)}
                >
                  {versionsList.map((v) => (
                    <option key={v.id} value={v.id}>
                      {v.version_number} — {formatDate(v.date_published)} ({formatBytes(v.files.find((f) => f.primary)?.size ?? 0)})
                    </option>
                  ))}
                </select>
              )}
            </div>
            <button
              className="btn primary"
              onClick={installSelected}
              disabled={busyCard === details.project_id || !pickedVersion}
            >
              <IconDownload size={15} />
              Установить {pickedVersion ? "" : "(выберите версию)"}
            </button>
          </div>
        </div>
      ) : null}
    </div>
  );
}
