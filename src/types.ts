export interface Account {
  id: string;
  kind: string;
  username: string;
  uuid: string;
  access_token: string;
  refresh_token: string;
  expires_at_ms: number;
}

export interface Config {
  root_dir: string;
  ram_mb: number;
  java_path: string;
  extra_jvm_args: string[];
  extra_game_args: string[];
  width: number;
  height: number;
  keep_launcher_open: boolean;
  ms_client_id: string;
  accounts: Account[];
  active_account_id: string | null;
  active_version: string | null;
  wallpaper: string;
  wallpaper_blur: number;
  wallpaper_dim: number;
  play_loader: string;
}

export interface ManifestVersion {
  id: string;
  kind: string;
  url: string;
  release_time: string;
  sha1: string | null;
}

export interface SizeBreakdown {
  client_bytes: number;
  libraries_bytes: number;
  assets_bytes: number;
  total_bytes: number;
}

export interface InstalledVersion {
  id: string;
  loader: string;
  has_jar: boolean;
}

export interface JavaCandidate {
  path: string;
  runtime: boolean;
}

export interface SearchHit {
  slug: string;
  title: string;
  project_id: string;
  project_type: string;
  author: string;
  description: string;
  downloads: number;
  icon_url: string | null;
  categories: string[];
  date_created: string;
  follows: number;
}

export interface SearchResponse {
  hits: SearchHit[];
  total_hits: number;
}

export interface Project {
  slug: string;
  title: string;
  project_id: string;
  description: string;
  body: string;
  project_type: string;
  downloads: number;
  icon_url: string | null;
  game_versions: string[];
  loaders: string[];
  categories: string[];
  license: { id?: string; name?: string } | null;
  date_created: string;
}

export interface ModFile {
  url: string;
  filename: string;
  primary: boolean;
  size: number;
  hashes: Record<string, string> | null;
}

export interface Dependency {
  version_id: string | null;
  project_id: string | null;
  dependency_type: string;
}

export interface ModVersion {
  id: string;
  project_id: string;
  name: string;
  version_number: string;
  game_versions: string[];
  loaders: string[];
  date_published: string;
  files: ModFile[];
  dependencies: Dependency[];
}

export interface FileInfo {
  name: string;
  size_bytes: number;
  is_dir: boolean;
}

export interface ProgressPayload {
  job_id: string;
  label: string;
  phase: string;
  total_bytes: number;
  downloaded_bytes: number;
  remaining_bytes: number;
  files_done: number;
  files_total: number;
  current_file: string;
  speed_bps: number;
}

export interface TaskFinished {
  job_id: string;
  kind: string;
  ok: boolean;
  message: string;
}

export interface ConsoleEntry {
  ts: string;
  level: string;
  source: string;
  message: string;
}

export interface LaunchInfo {
  pid: number;
  version: string;
  java: string;
  memory_mb: number;
}

export interface DeviceCode {
  device_code: string;
  user_code: string;
  verification_uri: string;
  expires_in: number;
  interval: number;
}

export interface RootInfo {
  path: string;
  exists: boolean;
  versions: number;
  mods: number;
  total_size_bytes: number;
}
