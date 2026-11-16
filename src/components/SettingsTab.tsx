import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api, formatBytes } from "../api";
import { t } from "../i18n";
import type { Config, DeviceCode, JavaCandidate, RootInfo, UpdateCheck } from "../types";
import {
  IconFolder,
  IconDownload,
  IconPlus,
  IconTrash,
  IconUser,
  IconCPU,
  IconExternal,
  IconCheck,
  IconImage,
  IconInfo,
  IconStats,
  IconRefresh,
} from "../icons";
import { Spinner } from "./common";

interface SettingsTabProps {
  config: Config;
  patchConfig: (patch: Partial<Config>) => Promise<void>;
  startJob: (jobId: string, label: string) => void;
  showToast: (msg: string, ok?: boolean) => void;
}

export default function SettingsTab({ config, patchConfig, startJob, showToast }: SettingsTabProps) {
  const [rootPath, setRootPath] = useState(config.root_dir);
  const [rootInfo, setRootInfo] = useState<RootInfo | null>(null);
  const [runtimes, setRuntimes] = useState<JavaCandidate[]>([]);
  const [newNick, setNewNick] = useState("");
  const [msClient, setMsClient] = useState(config.ms_client_id);
  const [deviceCode, setDeviceCode] = useState<DeviceCode | null>(null);
  const [msBusy, setMsBusy] = useState(false);
  const [launcherVersion, setLauncherVersion] = useState("");
  const [updateInfo, setUpdateInfo] = useState<UpdateCheck | null>(null);
  const [updateBusy, setUpdateBusy] = useState(false);

  useEffect(() => {
    api.getLauncherVersion().then(setLauncherVersion).catch(() => setLauncherVersion("1.0.1"));
  }, []);

  const autoRam = () => {
    api
      .getSystemRamMb()
      .then((totalMb) => {
        // четверть объёма ОЗУ, округлённая к 512 МБ, в пределах слайдера
        const raw = Math.round(totalMb / 4 / 512) * 512;
        const rec = Math.max(1024, Math.min(32768, raw));
        patchConfig({ ram_mb: rec });
        showToast(`${t("settings.ramRecommended")}: ${rec} МБ (ПК: ${formatBytes(totalMb * 1024 * 1024)})`, true);
      })
      .catch(() => showToast(t("settings.ramUnknown"), false));
  };

  const checkUpdates = () => {
    setUpdateBusy(true);
    api
      .checkUpdate()
      .then((u) => {
        setUpdateInfo(u);
        if (!u.update_available) {
          showToast(t("settings.upToDate"), true);
        } else {
          showToast(`${t("settings.newVersion")}: ${u.latest}`, true);
        }
      })
      .catch(() => showToast(t("settings.updateFailed"), false))
      .finally(() => setUpdateBusy(false));
  };

  useEffect(() => {
    setRootPath(config.root_dir);
    setMsClient(config.ms_client_id);
  }, [config.root_dir, config.ms_client_id]);

  const reloadRoot = () => {
    api.getRootInfo().then(setRootInfo).catch(() => {});
  };
  const reloadJava = () => {
    api.getJavaRuntimes().then(setRuntimes).catch(() => {});
  };

  useEffect(() => {
    reloadRoot();
    reloadJava();
  }, [config.root_dir]);

  const applyRoot = () => {
    api
      .setRootPath(rootPath)
      .then((cfg) => {
        showToast(`${t("settings.dirTitle")}: ${cfg.root_dir}`, true);
        reloadRoot();
      })
      .catch((e) => showToast(String(e), false));
  };

  const browse = async () => {
    const picked = await open({ directory: true, multiple: false });
    if (typeof picked === "string") {
      setRootPath(picked);
    }
  };

  const pickWallpaper = async () => {
    try {
      const picked = await open({
        directory: false,
        multiple: false,
        filters: [
          { name: "Изображения", extensions: ["png", "jpg", "jpeg", "webp", "gif", "bmp"] },
        ],
      });
      if (typeof picked === "string") {
        const cfg = await api.setCustomWallpaper(picked);
        patchConfig({ wallpaper: cfg.wallpaper });
        showToast(t("settings.wallpaperApplied"), true);
      }
    } catch (e) {
      showToast(String(e), false);
    }
  };

  const openRoot = () => {
    api.openFolder(config.root_dir).catch((e) => showToast(String(e), false));
  };

  const addOffline = () => {
    if (!newNick.trim()) return;
    api
      .addOfflineAccount(newNick.trim())
      .then((cfg) => {
        setNewNick("");
        patchConfig(cfg);
        showToast(t("settings.accountAdded"), true);
      })
      .catch((e) => showToast(String(e), false));
  };

  const startMsLogin = () => {
    if (!msClient.trim()) {
      showToast(t("settings.msClientNeeded"), false);
      return;
    }
    setMsBusy(true);
    patchConfig({ ms_client_id: msClient.trim() })
      .then(() => api.msLoginStart())
      .then(setDeviceCode)
      .catch((e) => showToast(String(e), false))
      .finally(() => setMsBusy(false));
  };

  const finishMsLogin = () => {
    if (!deviceCode) return;
    setMsBusy(true);
    api
      .msLoginFinish(deviceCode.device_code)
      .then((cfg) => {
        patchConfig(cfg);
        setDeviceCode(null);
        showToast("Вход через Microsoft выполнен", true);
      })
      .catch((e) => showToast(String(e), false))
      .finally(() => setMsBusy(false));
  };

  const installJava = (major: number) => {
    api
      .installJava(major)
      .then((jobId) => startJob(jobId, `Установка Java ${major} (Temurin JRE)`))
      .catch((e) => showToast(String(e), false));
  };

  const activeAccount = config.accounts.find((a) => a.id === config.active_account_id);

  return (
    <div>
      <div className="page-title">{t("settings.title")}</div>
      <div className="page-subtitle">{t("settings.subtitle")}</div>

      <div className="card">
        <div className="card-title">
          <IconFolder size={16} />
          {t("settings.dirTitle")}
        </div>
        <div className="row wrap">
          <input
            className="input"
            style={{ flex: 1, minWidth: 260 }}
            value={rootPath}
            onChange={(e) => setRootPath(e.target.value)}
            placeholder="C:\AstralLauncher"
          />
          <button className="btn" onClick={browse}>
            <IconFolder size={15} />
            {t("settings.browse")}
          </button>
          <button className="btn primary" onClick={applyRoot}>
            <IconCheck size={15} />
            {t("settings.apply")}
          </button>
          <button className="btn" onClick={openRoot}>
            <IconExternal size={15} />
            {t("settings.openFolder")}
          </button>
        </div>
        {rootInfo ? (
          <div className="hint" style={{ marginTop: 8 }}>
            {t("settings.versions")}: {rootInfo.versions} | {t("settings.mods")}: {rootInfo.mods} |{" "}
            {t("settings.totalSize")}: {formatBytes(rootInfo.total_size_bytes)}
          </div>
        ) : null}
        <div className="hint">{t("settings.dirHint")}</div>
      </div>

      <div className="card">
        <div className="card-title">
          <IconCPU size={16} />
          {t("settings.memoryJava")}
        </div>
        <div className="slider-row">
          <span style={{ color: "var(--text-faint)", fontSize: 12 }}>1024 МБ</span>
          <input
            type="range"
            min={1024}
            max={32768}
            step={512}
            value={config.ram_mb}
            onChange={(e) => patchConfig({ ram_mb: parseInt(e.target.value, 10) })}
          />
          <span style={{ color: "var(--text-faint)", fontSize: 12 }}>32 ГБ</span>
          <span className="slider-value">{config.ram_mb} МБ</span>
          <button className="btn sm" onClick={autoRam} title={t("settings.autoRam")}>
            <IconStats size={13} />
            {t("settings.autoRam")}
          </button>
        </div>
        <div className="divider" />
        <span className="label">{t("settings.javaPath")}</span>
        <div className="row wrap">
          <input
            className="input"
            style={{ flex: 1, minWidth: 260 }}
            value={config.java_path}
            onChange={(e) => patchConfig({ java_path: e.target.value })}
            placeholder="C:\Program Files\Java\jdk-17"
          />
        </div>
        <div style={{ marginTop: 10 }} className="row wrap">
          {runtimes.map((r, i) => (
            <span key={i} className={`badge ${r.runtime ? "release" : ""}`}>
              {r.path.split("\\").slice(-3).join("\\")}
            </span>
          ))}
          {runtimes.length === 0 ? <span className="hint">Java не найдена — установите ниже</span> : null}
        </div>
        <div className="row" style={{ marginTop: 10 }}>
          <button className="btn" onClick={() => installJava(8)}>
            <IconDownload size={14} />
            Java 8
          </button>
          <button className="btn" onClick={() => installJava(17)}>
            <IconDownload size={14} />
            Java 17
          </button>
          <button className="btn" onClick={() => installJava(21)}>
            <IconDownload size={14} />
            Java 21
          </button>
        </div>
        <div className="hint">
          Нужная мажорная версия определяется автоматически из манифеста версии игры. Свежие JRE
          скачиваются из Eclipse Adoptium и распаковываются в каталог установки.
        </div>
      </div>

      <div className="card">
        <div className="card-title">{t("settings.appearance")}</div>
        <div className="row wrap" style={{ alignItems: "flex-end" }}>
          <div>
            <span className="label">{t("settings.wallpaper")}</span>
            <select
              className="select"
              style={{ minWidth: 220 }}
              value={config.wallpaper || "aurora"}
              onChange={(e) => patchConfig({ wallpaper: e.target.value })}
            >
              <option value="aurora">{t("settings.wpAurora")}</option>
              <option value="nebula">{t("settings.wpNebula")}</option>
              <option value="ember">{t("settings.wpEmber")}</option>
              <option value="custom">{t("settings.wpCustom")}</option>
              <option value="none">{t("settings.wpNone")}</option>
            </select>
          </div>
          <div>
            <span className="label">{t("settings.language")}</span>
            <select
              className="select"
              value={config.language || "ru"}
              onChange={(e) => patchConfig({ language: e.target.value })}
            >
              <option value="ru">Русский</option>
              <option value="en">English</option>
            </select>
          </div>
          <button className="btn" onClick={pickWallpaper}>
            <IconImage size={15} />
            {t("settings.uploadWallpaper")}
          </button>
        </div>
        <div className="row wrap" style={{ marginTop: 14, gap: 24 }}>
          <div style={{ minWidth: 240 }}>
            <span className="label">{t("settings.blur")}: {config.wallpaper_blur ?? 18} px</span>
            <input
              type="range"
              min={0}
              max={40}
              step={2}
              value={config.wallpaper_blur ?? 18}
              onChange={(e) => patchConfig({ wallpaper_blur: Number(e.target.value) })}
              style={{ width: "100%" }}
            />
          </div>
          <div style={{ minWidth: 240 }}>
            <span className="label">{t("settings.dim")}: {config.wallpaper_dim ?? 40}%</span>
            <input
              type="range"
              min={10}
              max={80}
              step={5}
              value={config.wallpaper_dim ?? 40}
              onChange={(e) => patchConfig({ wallpaper_dim: Number(e.target.value) })}
              style={{ width: "100%" }}
            />
          </div>
        </div>
        <div className="row wrap" style={{ marginTop: 14, gap: 24 }}>
          <div style={{ minWidth: 240 }}>
            <span className="label">{t("settings.auroraIntensity")}: {config.aurora_intensity ?? 55}%</span>
            <input
              type="range"
              min={0}
              max={100}
              step={5}
              value={config.aurora_intensity ?? 55}
              onChange={(e) => patchConfig({ aurora_intensity: Number(e.target.value) })}
              style={{ width: "100%" }}
            />
          </div>
          <div style={{ minWidth: 240 }}>
            <span className="label">{t("settings.auroraHue")}: {config.aurora_hue ?? 0}°</span>
            <input
              type="range"
              min={-180}
              max={180}
              step={5}
              value={config.aurora_hue ?? 0}
              onChange={(e) => patchConfig({ aurora_hue: Number(e.target.value) })}
              style={{ width: "100%" }}
            />
          </div>
        </div>
        <div className="divider" />
        <span className="label">{t("settings.accentColor")}</span>
        <div className="row wrap" style={{ gap: 8, marginTop: 6 }}>
          {[
            { id: "violet", color: "#7b6cff", label: t("settings.accentViolet") },
            { id: "blue", color: "#4f8bff", label: t("settings.accentBlue") },
            { id: "green", color: "#2ec27e", label: t("settings.accentGreen") },
            { id: "pink", color: "#e35fb2", label: t("settings.accentPink") },
            { id: "amber", color: "#d99a3d", label: t("settings.accentAmber") },
          ].map((a) => (
            <button
              key={a.id}
              className={`accent-swatch ${(config.accent || "violet") === a.id ? "active" : ""}`}
              title={a.label}
              onClick={() => patchConfig({ accent: a.id })}
            >
              <span className="accent-dot" style={{ background: a.color }} />
              {a.label}
            </button>
          ))}
        </div>
        <div className="divider" />
        <label className="checkbox-row">
          <input
            type="checkbox"
            checked={config.sounds_enabled ?? true}
            onChange={(e) => patchConfig({ sounds_enabled: e.target.checked })}
          />
          {t("settings.sounds")}
        </label>
        <div className="hint">{t("settings.soundsHint")}</div>
        <div className="hint" style={{ marginTop: 8 }}>
          {t("settings.appearanceHint")}
        </div>
      </div>

      <div className="card">
        <div className="card-title">{t("settings.gameWindow")}</div>
        <div className="row wrap">
          <div>
            <span className="label">{t("settings.width")}</span>
            <input
              className="input"
              type="number"
              style={{ width: 120 }}
              value={config.width}
              onChange={(e) => patchConfig({ width: parseInt(e.target.value, 10) || 0 })}
            />
          </div>
          <div>
            <span className="label">{t("settings.height")}</span>
            <input
              className="input"
              type="number"
              style={{ width: 120 }}
              value={config.height}
              onChange={(e) => patchConfig({ height: parseInt(e.target.value, 10) || 0 })}
            />
          </div>
          <label className="checkbox-row" style={{ marginTop: 18 }}>
            <input
              type="checkbox"
              checked={config.keep_launcher_open}
              onChange={(e) => patchConfig({ keep_launcher_open: e.target.checked })}
            />
            {t("settings.keepOpen")}
          </label>
        </div>
      </div>

      <div className="card">
        <div className="card-title">{t("settings.args")}</div>
        <div className="row wrap" style={{ gap: 14 }}>
          <div style={{ flex: 1, minWidth: 240 }}>
            <span className="label">{t("settings.jvmArgs")}</span>
            <textarea
              className="input"
              style={{ width: "100%" }}
              value={config.extra_jvm_args.join("\n")}
              onChange={(e) =>
                patchConfig({
                  extra_jvm_args: e.target.value.split("\n").filter((s) => s.trim() !== ""),
                })
              }
            />
          </div>
          <div style={{ flex: 1, minWidth: 240 }}>
            <span className="label">{t("settings.gameArgs")}</span>
            <textarea
              className="input"
              style={{ width: "100%" }}
              value={config.extra_game_args.join("\n")}
              onChange={(e) =>
                patchConfig({
                  extra_game_args: e.target.value.split("\n").filter((s) => s.trim() !== ""),
                })
              }
            />
          </div>
        </div>
      </div>

      <div className="card">
        <div className="card-title">
          <IconUser size={16} />
          {t("settings.accounts")}
        </div>
        {config.accounts.length > 0 ? (
          <div className="list" style={{ maxHeight: 220, marginBottom: 12 }}>
            {config.accounts.map((a) => (
              <div
                key={a.id}
                className={`list-item ${a.id === config.active_account_id ? "selected" : ""}`}
                onClick={() => api.setActiveAccount(a.id).then(patchConfig).catch(() => {})}
              >
                <div className="sidebar-account" style={{ width: 32, height: 32, fontSize: 13 }}>
                  {a.username.slice(0, 1).toUpperCase()}
                </div>
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div style={{ fontWeight: 600, fontSize: 13 }}>{a.username}</div>
                  <div className="meta">{a.kind === "microsoft" ? "Microsoft" : "Офлайн"}</div>
                </div>
                {a.id === config.active_account_id ? <span className="badge release">{t("settings.active")}</span> : null}
                <button
                  className="btn sm danger"
                  title={t("settings.delete")}
                  onClick={(e) => {
                    e.stopPropagation();
                    api.removeAccount(a.id).then(patchConfig).catch(() => {});
                  }}
                >
                  <IconTrash size={13} />
                </button>
              </div>
            ))}
          </div>
        ) : (
          <div className="hint" style={{ marginBottom: 12 }}>
            {t("settings.noAccounts")}
          </div>
        )}
        <div className="row wrap" style={{ marginBottom: 8 }}>
          <input
            className="input"
            style={{ width: 200 }}
            placeholder={t("settings.nickPlaceholder")}
            value={newNick}
            onChange={(e) => setNewNick(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") addOffline();
            }}
          />
          <button className="btn" onClick={addOffline}>
            <IconPlus size={14} />
            {t("settings.addOffline")}
          </button>
        </div>
        <div className="divider" />
        <span className="label">{t("settings.msClientId")}</span>
        <div className="row wrap">
          <input
            className="input"
            style={{ flex: 1, minWidth: 240 }}
            value={msClient}
            onChange={(e) => setMsClient(e.target.value)}
            placeholder="00000000-0000-0000-0000-000000000000"
          />
          <button className="btn primary" onClick={startMsLogin} disabled={msBusy}>
            {msBusy ? <Spinner /> : null}
            {t("settings.msLogin")}
          </button>
        </div>
        <div className="hint">
          {t("settings.msHint")}
        </div>
        {activeAccount?.kind === "microsoft" ? (
          <div className="hint">
            Токен {activeAccount.username}: {activeAccount.expires_at_ms > Date.now() ? "действителен" : "истёк, войдите заново"}
          </div>
        ) : null}
      </div>

      <div className="card">
        <div className="card-title">
          <IconInfo size={16} />
          {t("settings.about")}
        </div>
        <div className="row" style={{ justifyContent: "space-between", alignItems: "center" }}>
          <div>
            <div style={{ fontWeight: 700, fontSize: 14 }}>AstralLauncher {launcherVersion}</div>
            <div className="meta" style={{ color: "var(--text-faint)", fontSize: 12 }}>
              Minecraft launcher · 2026
            </div>
          </div>
          <button className="btn" onClick={checkUpdates} disabled={updateBusy}>
            {updateBusy ? <Spinner /> : <IconRefresh size={14} />}
            {t("settings.checkUpdates")}
          </button>
        </div>
        {updateInfo && updateInfo.update_available ? (
          <div className="hint" style={{ marginTop: 8, color: "var(--warn)" }}>
            {t("settings.newVersion")}: {updateInfo.latest}
            {updateInfo.notes ? ` — ${updateInfo.notes}` : ""}
            {updateInfo.url ? (
              <>
                {" · "}
                <a className="link" href={updateInfo.url} target="_blank" rel="noreferrer">
                  <IconExternal size={12} /> {updateInfo.url}
                </a>
              </>
            ) : null}
          </div>
        ) : null}
      </div>

      {deviceCode ? (
        <div className="modal-backdrop">
          <div className="modal">
            <h3>{t("settings.msModalTitle")}</h3>
            <div className="desc">
              {t("settings.msModalDesc")}
            </div>
            <div className="code-box">{deviceCode.user_code}</div>
            <a className="link" href={deviceCode.verification_uri} target="_blank" rel="noreferrer">
              <IconExternal size={14} />
              {deviceCode.verification_uri}
            </a>
            <div className="row" style={{ justifyContent: "flex-end" }}>
              <button className="btn" onClick={() => setDeviceCode(null)}>
                Отмена
              </button>
              <button className="btn primary" onClick={finishMsLogin} disabled={msBusy}>
                {msBusy ? <Spinner /> : <IconCheck size={14} />}
                {t("settings.msEntered")}
              </button>
            </div>
          </div>
        </div>
      ) : null}
    </div>
  );
}
