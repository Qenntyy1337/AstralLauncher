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
} from "./types";

export const api = {
  getConfig: () => invoke<Config>("get_config"),
  updateConfig: (cfg: Config) => invoke<Config>("update_config", { cfg }),
  setRootPath: (path: string) => invoke<Config>("set_root_path", { path }),
  getRootInfo: () => invoke<RootInfo>("get_root_info"),
  openFolder: (path: string) => invoke<void>("open_folder", { path }),
  listVersions: () => invoke<ManifestVersion[]>("list_versions"),
  versionSize: (id: string) => invoke<SizeBreakdown>("version_size", { id }),
  listInstalled: () => invoke<InstalledVersion[]>("list_installed_versions"),
  installVersion: (id: string, loader: string | null, loaderVersion: string | null) =>
    invoke<string>("install_version_cmd", { id, loader, loaderVersion }),
  loaderVersions: (loader: string, mc: string) =>
    invoke<string[]>("loader_versions", { loader, mc }),
  installJava: (major: number) => invoke<string>("install_java_cmd", { major }),
  getJavaRuntimes: () => invoke<JavaCandidate[]>("get_java_runtimes"),
  setActiveVersion: (id: string) => invoke<Config>("set_active_version", { id }),
  addOfflineAccount: (username: string) => invoke<Config>("add_offline_account", { username }),
  msLoginStart: () => invoke<DeviceCode>("ms_login_start"),
  msLoginFinish: (deviceCode: string) => invoke<Config>("ms_login_finish_cmd", { deviceCode }),
  removeAccount: (id: string) => invoke<Config>("remove_account", { id }),
  setActiveAccount: (id: string) => invoke<Config>("set_active_account", { id }),
  modrinthSearch: (
    query: string,
    projectType: string | null,
    mcVersion: string | null,
    loader: string | null,
    offset: number,
  ) =>
    invoke<SearchResponse>("modrinth_search_cmd", {
      query,
      projectType,
      mcVersion,
      loader,
      categories: null,
      offset,
    }),
  modrinthProject: (projectId: string) => invoke<Project>("modrinth_project_cmd", { projectId }),
  modrinthVersions: (projectId: string, mcVersion: string | null, loader: string | null) =>
    invoke<ModVersion[]>("modrinth_versions_cmd", { projectId, mcVersion, loader }),
  modrinthInstall: (
    projectId: string,
    projectTitle: string,
    projectType: string,
    versionId: string | null,
    mcVersion: string,
    loader: string,
  ) =>
    invoke<string>("modrinth_install_cmd", {
      projectId,
      projectTitle,
      projectType,
      versionId,
      mcVersion,
      loader,
    }),
  listContentFiles: (projectType: string) =>
    invoke<FileInfo[]>("list_content_files_cmd", { projectType }),
  deleteContentFile: (projectType: string, name: string) =>
    invoke<void>("delete_content_file_cmd", { projectType, name }),
  launchGame: (versionId: string) => invoke<LaunchInfo>("launch_game", { versionId }),
  stopGame: (pid: number) => invoke<void>("stop_game_cmd", { pid }),
  cancelJob: (jobId: string) => invoke<boolean>("cancel_job", { jobId }),
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

export function formatDate(iso: string): string {
  if (!iso) return "";
  const d = new Date(iso);
  if (isNaN(d.getTime())) return "";
  return d.toLocaleDateString("ru-RU", { day: "2-digit", month: "2-digit", year: "numeric" });
}

export function formatNumber(n: number): string {
  return n.toLocaleString("ru-RU");
}
