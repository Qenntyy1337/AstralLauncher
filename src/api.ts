import { invoke } from "@tauri-apps/api/core";
import type {
  Config,
  ManifestVersion,
  SizeBreakdown,
  InstalledVersion,
  JavaCandidate,
  SearchResponse,
  Project,
  ModVersion,
  FileInfo,
  DeviceCode,
  RootInfo,
  LaunchInfo,
  PlayStats,
  UpdateCheck,
} from "./types";

/**
 * Tauri по умолчанию ждёт camelCase-ключи аргументов, но дублируем их и в
 * snake_case (лишние ключи бэкенд игнорирует), а undefined не отправляем —
 * JSON.stringify выкинул бы такой ключ, и команда упала бы с «missing required key».
 */
function call<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  const out: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(args)) {
    if (v === undefined) continue;
    out[k] = v;
    const snake = k.replace(/[A-Z]/g, (c) => `_${c.toLowerCase()}`);
    if (snake !== k && !(snake in out)) out[snake] = v;
  }
  return invoke<T>(cmd, out);
}

export const api = {
  getConfig: () => call<Config>("get_config"),
  updateConfig: (cfg: Config) => call<Config>("update_config", { cfg }),
  setRootPath: (path: string) => call<Config>("set_root_path", { path }),
  getRootInfo: () => call<RootInfo>("get_root_info"),
  openFolder: (path: string) => call<void>("open_folder", { path }),
  listVersions: () => call<ManifestVersion[]>("list_versions"),
  versionSize: (id: string) => call<SizeBreakdown>("version_size", { id }),
  listInstalled: () => call<InstalledVersion[]>("list_installed_versions"),
  installVersion: (id: string, loader: string | null, loaderVersion: string | null) =>
    call<string>("install_version_cmd", { id, loader, loaderVersion }),
  loaderVersions: (loader: string, mc: string) =>
    call<string[]>("loader_versions", { loader, mc }),
  installJava: (major: number) => call<string>("install_java_cmd", { major }),
  getJavaRuntimes: () => call<JavaCandidate[]>("get_java_runtimes"),
  setActiveVersion: (id: string) => call<Config>("set_active_version", { id }),
  addOfflineAccount: (username: string) => call<Config>("add_offline_account", { username }),
  msLoginStart: () => call<DeviceCode>("ms_login_start"),
  msLoginFinish: (deviceCode: string) => call<Config>("ms_login_finish_cmd", { deviceCode }),
  removeAccount: (id: string) => call<Config>("remove_account", { id }),
  setActiveAccount: (id: string) => call<Config>("set_active_account", { id }),
  modrinthSearch: (
    query: string,
    projectType: string | null,
    mcVersion: string | null,
    loader: string | null,
    offset: number,
  ) =>
    call<SearchResponse>("modrinth_search_cmd", {
      query,
      projectType,
      mcVersion,
      loader,
      categories: null,
      offset,
    }),
  modrinthProject: (projectId: string) => call<Project>("modrinth_project_cmd", { projectId }),
  modrinthVersions: (projectId: string, mcVersion: string | null, loader: string | null) =>
    call<ModVersion[]>("modrinth_versions_cmd", { projectId, mcVersion, loader }),
  modrinthInstall: (
    projectId: string,
    projectTitle: string,
    projectType: string,
    versionId: string | null,
    mcVersion: string,
    loader: string,
  ) =>
    call<string>("modrinth_install_cmd", {
      projectId,
      projectTitle,
      projectType,
      versionId,
      mcVersion,
      loader,
    }),
  listContentFiles: (projectType: string) =>
    call<FileInfo[]>("list_content_files_cmd", { projectType }),
  deleteContentFile: (projectType: string, name: string) =>
    call<void>("delete_content_file_cmd", { projectType, name }),
  launchGame: (versionId: string) => call<LaunchInfo>("launch_game", { versionId }),
  stopGame: (pid: number) => call<void>("stop_game_cmd", { pid }),
  cancelJob: (jobId: string) => call<boolean>("cancel_job", { jobId }),
  setCustomWallpaper: (path: string) => call<Config>("set_custom_wallpaper", { path }),
  getCustomWallpaper: () => call<string | null>("get_custom_wallpaper"),
  getPlayStats: () => call<PlayStats>("get_play_stats"),
  getLauncherVersion: () => call<string>("get_launcher_version"),
  checkUpdate: () => call<UpdateCheck>("check_launcher_update"),
  getSystemRamMb: () => call<number>("get_system_ram_mb"),
  revealPath: (path: string) => call<void>("reveal_path", { path }),
  openLogsDir: () => call<void>("open_logs_dir"),
  installLocalMod: (path: string) => call<string>("install_local_mod_cmd", { path }),
  installMrpack: (path: string) => call<string>("install_mrpack_cmd", { path }),
};

export function formatBytes(n: number): string {
  if (n >= 1024 * 1024 * 1024) {
    return `${(n / (1024 * 1024 * 1024)).toFixed(2)} ГБ`;
  }
  if (n >= 1024 * 1024) {
    return `${(n / (1024 * 1024)).toFixed(1)} МБ`;
  }
  if (n >= 1024) {
    return `${Math.round(n / 1024)} КБ`;
  }
  return `${n} Б`;
}

export function formatSpeed(bps: number): string {
  if (bps <= 0) return "";
  return `${formatBytes(bps)}/с`;
}

export function formatEta(sec: number): string {
  if (sec <= 0 || !isFinite(sec)) return "";
  if (sec >= 3600) {
    return `${Math.floor(sec / 3600)} ч ${Math.floor((sec % 3600) / 60)} мин`;
  }
  if (sec >= 60) {
    return `${Math.floor(sec / 60)} мин ${sec % 60} с`;
  }
  return `${sec} с`;
}

export function formatDate(iso: string): string {
  if (!iso) return "";
  const d = new Date(iso);
  if (isNaN(d.getTime())) return "";
  return d.toLocaleDateString("ru-RU", { day: "2-digit", month: "2-digit", year: "numeric" });
}

export function formatNumber(n: number): string {
  return n.toLocaleString("ru-RU");
}
