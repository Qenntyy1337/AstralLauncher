import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api, formatBytes } from "../api";
import type { Config, DeviceCode, JavaCandidate, RootInfo } from "../types";
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
        showToast(`Каталог установки: ${cfg.root_dir}`, true);
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
        showToast("Обои применены", true);
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
        showToast("Аккаунт добавлен", true);
      })
      .catch((e) => showToast(String(e), false));
  };

  const startMsLogin = () => {
    if (!msClient.trim()) {
      showToast("Сначала укажите MS Azure client_id", false);
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
      <div className="page-title">Настройки</div>
      <div className="page-subtitle">Каталог установки, память, Java и аккаунты</div>

      <div className="card">
        <div className="card-title">
          <IconFolder size={16} />
          Каталог установки
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
            Обзор
          </button>
          <button className="btn primary" onClick={applyRoot}>
            <IconCheck size={15} />
            Применить
          </button>
          <button className="btn" onClick={openRoot}>
            <IconExternal size={15} />
            Открыть папку
          </button>
        </div>
        {rootInfo ? (
          <div className="hint" style={{ marginTop: 8 }}>
            Версий: {rootInfo.versions} | Модов: {rootInfo.mods} | Общий объём:{" "}
            {formatBytes(rootInfo.total_size_bytes)}
          </div>
        ) : null}
        <div className="hint">
          Все версии, библиотеки, ассеты, моды и конфигурация хранятся здесь. При смене пути новые
          файлы будут устанавливаться в новый каталог, конфигурация переносится автоматически.
        </div>
      </div>

      <div className="card">
        <div className="card-title">
          <IconCPU size={16} />
          Память и Java
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
        </div>
        <div className="divider" />
        <span className="label">Путь к Java (пусто = автоопределение)</span>
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
        <div className="card-title">Внешний вид</div>
        <div className="row wrap" style={{ alignItems: "flex-end" }}>
          <div>
            <span className="label">Фон лаунчера</span>
            <select
              className="select"
              style={{ minWidth: 220 }}
              value={config.wallpaper || "aurora"}
              onChange={(e) => patchConfig({ wallpaper: e.target.value })}
            >
              <option value="aurora">Полярное сияние</option>
              <option value="nebula">Туманность</option>
              <option value="ember">Угольки</option>
              <option value="custom">Свои обои</option>
              <option value="none">Без обоев</option>
            </select>
          </div>
          <button className="btn" onClick={pickWallpaper}>
            <IconImage size={15} />
            Загрузить свои обои…
          </button>
        </div>
        <div className="row wrap" style={{ marginTop: 14, gap: 24 }}>
          <div style={{ minWidth: 240 }}>
            <span className="label">Размытие фона: {config.wallpaper_blur ?? 18} px</span>
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
            <span className="label">Затемнение фона: {config.wallpaper_dim ?? 40}%</span>
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
        <div className="hint" style={{ marginTop: 8 }}>
          Свои обои (PNG/JPG/WebP/GIF до 12 МБ) копируются в каталог лаунчера и применяются сразу.
          Слайдеры регулируют силу размытия и затемнения, чтобы текст оставался читаемым.
        </div>
      </div>

      <div className="card">
        <div className="card-title">Окно игры</div>
        <div className="row wrap">
          <div>
            <span className="label">Ширина (0 = авто)</span>
            <input
              className="input"
              type="number"
              style={{ width: 120 }}
              value={config.width}
              onChange={(e) => patchConfig({ width: parseInt(e.target.value, 10) || 0 })}
            />
          </div>
          <div>
            <span className="label">Высота</span>
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
            Держать лаунчер открытым во время игры
          </label>
        </div>
      </div>

      <div className="card">
        <div className="card-title">Аргументы запуска</div>
        <div className="row wrap" style={{ gap: 14 }}>
          <div style={{ flex: 1, minWidth: 240 }}>
            <span className="label">Дополнительные JVM-аргументы (по одному в строке)</span>
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
            <span className="label">Дополнительные аргументы игры</span>
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
          Аккаунты
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
                {a.id === config.active_account_id ? <span className="badge release">активен</span> : null}
                <button
                  className="btn sm danger"
                  title="Удалить аккаунт"
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
            Нет аккаунтов. Добавьте офлайн-аккаунт или войдите через Microsoft.
          </div>
        )}
        <div className="row wrap" style={{ marginBottom: 8 }}>
          <input
            className="input"
            style={{ width: 200 }}
            placeholder="Ник для офлайн-режима"
            value={newNick}
            onChange={(e) => setNewNick(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") addOffline();
            }}
          />
          <button className="btn" onClick={addOffline}>
            <IconPlus size={14} />
            Добавить офлайн
          </button>
        </div>
        <div className="divider" />
        <span className="label">MS Azure client_id (для входа через Microsoft)</span>
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
            Войти через Microsoft
          </button>
        </div>
        <div className="hint">
          Вход выполняется через официальное device code flow: введите код на странице Microsoft,
          затем нажмите «Я ввёл код». Токены хранятся локально в конфигурации лаунчера.
        </div>
        {activeAccount?.kind === "microsoft" ? (
          <div className="hint">
            Токен {activeAccount.username}: {activeAccount.expires_at_ms > Date.now() ? "действителен" : "истёк, войдите заново"}
          </div>
        ) : null}
      </div>

      {deviceCode ? (
        <div className="modal-backdrop">
          <div className="modal">
            <h3>Вход через Microsoft</h3>
            <div className="desc">
              Откройте страницу и введите код. Затем нажмите кнопку ниже.
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
                Я ввёл код
              </button>
            </div>
          </div>
        </div>
      ) : null}
    </div>
  );
}
