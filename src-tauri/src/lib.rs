use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};

mod accounts;
mod config;
mod download;
mod http;
mod java;
mod launch;
mod loaders;
mod logger;
mod modrinth;
mod state;
mod stats;
mod util;
mod version;

use crate::config::Config;
use crate::download::TaskFinished;
use crate::state::{AppState, JobState};

#[derive(Debug, Clone, Serialize)]
pub struct RootInfo {
    pub path: String,
    pub exists: bool,
    pub versions: usize,
    pub mods: usize,
    pub total_size_bytes: u64,
}

fn spawn_job<F>(app: &AppHandle, state: &AppState, kind: &str, f: F) -> Result<String, String>
where
    F: FnOnce(AppHandle, Arc<JobState>, String) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, String>> + Send>> + Send + 'static,
{
    let job_id = util::unique_id("job");
    let job = state.register_job(&job_id);
    let app2 = app.clone();
    let kind = kind.to_string();
    let job_id_task = job_id.clone();
    tokio::spawn(async move {
        let result = f(app2.clone(), job, job_id_task.clone()).await;
        let message = match &result {
            Ok(m) => m.clone(),
            Err(e) => {
                if e == "cancelled" {
                    "Задача отменена".to_string()
                } else {
                    e.clone()
                }
            }
        };
        let _ = app2.emit(
            "task-finished",
            TaskFinished {
                job_id: job_id_task.clone(),
                kind,
                ok: result.is_ok(),
                message,
            },
        );
        app2.state::<AppState>().drop_job(&job_id_task);
    });
    Ok(job_id)
}

#[tauri::command]
fn get_config(state: State<'_, AppState>) -> Config {
    state.config.lock().unwrap().clone()
}

#[tauri::command]
fn update_config(state: State<'_, AppState>, cfg: Config) -> Result<Config, String> {
    cfg.ensure_layout()?;
    cfg.save()?;
    *state.config.lock().unwrap() = cfg.clone();
    Ok(cfg)
}

#[tauri::command]
fn set_root_path(state: State<'_, AppState>, path: String) -> Result<Config, String> {
    let path = path.trim().to_string();
    if path.len() < 3 || !path.contains(':') {
        return Err("укажите путь вида C:\\AstralLauncher".to_string());
    }
    let mut cfg = state.config.lock().unwrap().clone();
    let old_root = cfg.root();
    let new_root = PathBuf::from(&path);
    std::fs::create_dir_all(&new_root).map_err(|e| format!("{}: {}", path, e))?;
    let old_config = Config::config_path(&old_root);
    if old_config != Config::config_path(&new_root) && old_config.is_file() {
        let text = std::fs::read_to_string(&old_config).unwrap_or_default();
        std::fs::write(Config::config_path(&new_root), text).map_err(|e| e.to_string())?;
        let _ = std::fs::remove_file(&old_config);
    }
    cfg.root_dir = path;
    cfg.ensure_layout()?;
    cfg.save()?;
    *state.config.lock().unwrap() = cfg.clone();
    Ok(cfg)
}

#[tauri::command]
fn get_root_info(state: State<'_, AppState>) -> RootInfo {
    let cfg = state.config.lock().unwrap().clone();
    let root = cfg.root();
    let versions = version::list_installed(&root).len();
    let mods = modrinth::list_content_files(&root, "mod")
        .map(|l| l.len())
        .unwrap_or(0);
    RootInfo {
        path: cfg.root_dir.clone(),
        exists: root.is_dir(),
        versions,
        mods,
        total_size_bytes: util::dir_size(&root),
    }
}

#[tauri::command]
fn open_folder(path: String) -> Result<(), String> {
    #[cfg(windows)]
    {
        std::process::Command::new("explorer")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err("доступно только на Windows".to_string())
    }
}

#[tauri::command]
async fn list_versions(app: AppHandle) -> Result<Vec<version::ManifestVersion>, String> {
    version::fetch_manifest_cached(&app).await
}

#[tauri::command]
async fn version_size(state: State<'_, AppState>, id: String) -> Result<version::SizeBreakdown, String> {
    version::version_meta_size(state.inner(), &id).await
}

#[tauri::command]
fn list_installed_versions(state: State<'_, AppState>) -> Vec<version::InstalledVersion> {
    let root = state.config.lock().unwrap().root();
    version::list_installed(&root)
}

#[tauri::command]
async fn install_version_cmd(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    loader: Option<String>,
    loader_version: Option<String>,
) -> Result<String, String> {
    cfg_check(&app)?;
    let root = state.config.lock().unwrap().root();
    spawn_job(&app, state.inner(), "install-version", move |app, job, job_id| {
        Box::pin(async move {
            let res = match loader {
                Some(ld) if ld != "vanilla" => {
                    loaders::install_loader(&app, &root, &job, &job_id, &ld, &id, loader_version.as_deref())
                        .await
                }
                _ => version::install_version(&app, &root, &job, &job_id, &id, None).await,
            };
            // Успешная установка делает версию активной (для загрузчика это его id,
            // например fabric-loader-0.20.1-26.3)
            if let Ok(installed_id) = &res {
                let st = app.state::<AppState>();
                let mut guard = st.config.lock().unwrap();
                guard.active_version = Some(installed_id.clone());
                let _ = guard.save();
            }
            res
        })
    })
}

#[tauri::command]
async fn loader_versions(loader: String, mc: String) -> Result<Vec<String>, String> {
    match loader.as_str() {
        "fabric" => loaders::fabric_loader_versions().await,
        "forge" => loaders::forge_versions(&mc).await,
        _ => Ok(Vec::new()),
    }
}

#[tauri::command]
async fn install_java_cmd(
    app: AppHandle,
    state: State<'_, AppState>,
    major: u32,
) -> Result<String, String> {
    let root = state.config.lock().unwrap().root();
    spawn_job(&app, state.inner(), "install-java", move |app, job, job_id| {
        Box::pin(async move {
            java::install_runtime(&app, &root, &job, &job_id, major)
                .await
                .map(|p| p.to_string_lossy().to_string())
        })
    })
}

#[tauri::command]
fn get_java_runtimes(state: State<'_, AppState>) -> Vec<java::JavaCandidate> {
    let cfg = state.config.lock().unwrap().clone();
    let root = cfg.root();
    java::list_java_candidates(&cfg, &root)
}

#[tauri::command]
fn set_active_version(state: State<'_, AppState>, id: String) -> Result<Config, String> {
    let mut cfg = state.config.lock().unwrap().clone();
    cfg.active_version = Some(id);
    cfg.save()?;
    *state.config.lock().unwrap() = cfg.clone();
    Ok(cfg)
}

fn cfg_check(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let cfg = state.config.lock().unwrap().clone();
    cfg.ensure_layout()
}

#[tauri::command]
fn add_offline_account(state: State<'_, AppState>, username: String) -> Result<Config, String> {
    let mut cfg = state.config.lock().unwrap().clone();
    let account = accounts::add_offline(&mut cfg, &username)?;
    cfg.active_account_id = Some(account.id.clone());
    cfg.save()?;
    *state.config.lock().unwrap() = cfg.clone();
    Ok(cfg)
}

#[tauri::command]
async fn ms_login_start(state: State<'_, AppState>) -> Result<accounts::DeviceCodeResponse, String> {
    let client_id = state.config.lock().unwrap().ms_client_id.clone();
    accounts::ms_login_start(&client_id).await
}

#[tauri::command]
async fn ms_login_finish_cmd(
    app: AppHandle,
    state: State<'_, AppState>,
    device_code: String,
) -> Result<Config, String> {
    let cfg = state.config.lock().unwrap().clone();
    let account = accounts::ms_login_finish(&cfg.ms_client_id, &device_code).await?;
    let mut updated = cfg.clone();
    updated.accounts.push(account.clone());
    updated.active_account_id = Some(account.id);
    updated.save()?;
    *state.config.lock().unwrap() = updated.clone();
    logger::emit_console(&app, "info", "accounts", &format!("Вход Microsoft выполнен: {}", updated.active_account().map(|a| a.username.clone()).unwrap_or_default()));
    Ok(updated)
}

#[tauri::command]
fn remove_account(state: State<'_, AppState>, id: String) -> Result<Config, String> {
    let mut cfg = state.config.lock().unwrap().clone();
    cfg.accounts.retain(|a| a.id != id);
    if cfg.active_account_id.as_deref() == Some(&id) {
        cfg.active_account_id = cfg.accounts.first().map(|a| a.id.clone());
    }
    cfg.save()?;
    *state.config.lock().unwrap() = cfg.clone();
    Ok(cfg)
}

#[tauri::command]
fn set_active_account(state: State<'_, AppState>, id: String) -> Result<Config, String> {
    let mut cfg = state.config.lock().unwrap().clone();
    if cfg.account_by_id(&id).is_none() {
        return Err("аккаунт не найден".to_string());
    }
    cfg.active_account_id = Some(id);
    cfg.save()?;
    *state.config.lock().unwrap() = cfg.clone();
    Ok(cfg)
}

#[tauri::command]
async fn modrinth_search_cmd(
    query: String,
    project_type: Option<String>,
    mc_version: Option<String>,
    loader: Option<String>,
    categories: Option<Vec<String>>,
    offset: Option<u32>,
) -> Result<modrinth::SearchResponse, String> {
    modrinth::search(
        &query,
        project_type.as_deref(),
        mc_version.as_deref(),
        loader.as_deref(),
        categories.as_deref().unwrap_or(&[]),
        offset.unwrap_or(0),
    )
    .await
}

#[tauri::command]
async fn modrinth_project_cmd(project_id: String) -> Result<modrinth::Project, String> {
    modrinth::project_details(&project_id).await
}

#[tauri::command]
async fn modrinth_versions_cmd(
    project_id: String,
    mc_version: Option<String>,
    loader: Option<String>,
) -> Result<Vec<modrinth::ModVersion>, String> {
    modrinth::project_versions(
        &project_id,
        mc_version.as_deref(),
        loader.as_deref(),
    )
    .await
}

#[tauri::command]
async fn modrinth_install_cmd(
    app: AppHandle,
    state: State<'_, AppState>,
    project_id: String,
    project_title: String,
    project_type: String,
    version_id: Option<String>,
    mc_version: Option<String>,
    loader: String,
) -> Result<String, String> {
    let root = state.config.lock().unwrap().root();
    spawn_job(&app, state.inner(), "install-content", move |app, job, job_id| {
        Box::pin(async move {
            modrinth::install_content(
                &app,
                &root,
                &job,
                &job_id,
                &project_id,
                &project_title,
                &project_type,
                version_id.as_deref(),
                mc_version.as_deref().unwrap_or(""),
                &loader,
            )
            .await
        })
    })
}

#[tauri::command]
fn list_content_files_cmd(
    state: State<'_, AppState>,
    project_type: String,
) -> Result<Vec<modrinth::FileInfo>, String> {
    let root = state.config.lock().unwrap().root();
    modrinth::list_content_files(&root, &project_type)
}

#[tauri::command]
fn delete_content_file_cmd(
    state: State<'_, AppState>,
    project_type: String,
    name: String,
) -> Result<(), String> {
    let root = state.config.lock().unwrap().root();
    modrinth::delete_content_file(&root, &project_type, &name)
}

#[tauri::command]
async fn launch_game(
    app: AppHandle,
    state: State<'_, AppState>,
    version_id: String,
) -> Result<launch::LaunchInfo, String> {
    launch::launch(&app, state.inner(), &version_id).await
}

#[tauri::command]
fn stop_game_cmd(pid: u32) -> Result<(), String> {
    launch::stop_game(pid)
}

#[tauri::command]
fn cancel_job(state: State<'_, AppState>, job_id: String) -> bool {
    state.cancel_job(&job_id)
}

// ===== 1.0.1: статистика, диагностика, локальные моды, модпаки, обновления =====

#[tauri::command]
fn get_play_stats(state: State<'_, AppState>) -> stats::PlayStats {
    let root = state.config.lock().unwrap().root();
    stats::load(&root)
}

#[tauri::command]
fn get_launcher_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateCheck {
    pub current: String,
    pub latest: String,
    pub update_available: bool,
    pub notes: String,
    pub url: String,
}

// Манифест обновлений: {"latest": "1.0.2", "notes": "...", "url": "https://..."}
const UPDATE_MANIFEST: &str =
    "https://raw.githubusercontent.com/astrallauncher/astrallauncher/main/latest.json";

#[tauri::command]
async fn check_launcher_update() -> Result<UpdateCheck, String> {
    let value = http::get_json(UPDATE_MANIFEST).await?;
    let latest = value
        .get("latest")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let notes = value
        .get("notes")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let url = value
        .get("url")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let current = env!("CARGO_PKG_VERSION").to_string();
    let parse = |s: &str| -> Vec<u32> {
        s.split('.')
            .map(|p| p.trim().parse().unwrap_or(0))
            .collect()
    };
    let (c, l) = (parse(&current), parse(&latest));
    let update_available = !latest.is_empty()
        && (l.len() > c.len() || c.iter().zip(l.iter()).any(|(a, b)| b > a));
    Ok(UpdateCheck {
        current,
        latest,
        update_available,
        notes,
        url,
    })
}

#[tauri::command]
fn get_system_ram_mb() -> Result<u64, String> {
    #[cfg(target_os = "windows")]
    {
        let out = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "(Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory",
            ])
            .creation_flags_cfg()
            .output()
            .map_err(|e| e.to_string())?;
        let text = String::from_utf8_lossy(&out.stdout);
        let bytes: u64 = text.trim().parse().map_err(|_| "не удалось определить ОЗУ".to_string())?;
        Ok(bytes / (1024 * 1024))
    }
    #[cfg(not(target_os = "windows"))]
    {
        let text = std::fs::read_to_string("/proc/meminfo").map_err(|e| e.to_string())?;
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("MemTotal:") {
                let kb: u64 = rest
                    .trim()
                    .trim_end_matches("kB")
                    .trim()
                    .parse()
                    .unwrap_or(0);
                return Ok(kb / 1024);
            }
        }
        Err("не удалось определить ОЗУ".to_string())
    }
}

trait CreationFlagsLib {
    fn creation_flags_cfg(&mut self) -> &mut Self;
}

#[cfg(windows)]
impl CreationFlagsLib for std::process::Command {
    fn creation_flags_cfg(&mut self) -> &mut Self {
        use std::os::windows::process::CommandExt;
        self.creation_flags(0x08000000)
    }
}

#[cfg(not(windows))]
impl CreationFlagsLib for std::process::Command {
    fn creation_flags_cfg(&mut self) -> &mut Self {
        self
    }
}

#[tauri::command]
fn reveal_path(path: String) -> Result<(), String> {
    #[cfg(windows)]
    {
        std::process::Command::new("explorer")
            .args(["/select,", &path])
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err("доступно только на Windows".to_string())
    }
}

#[tauri::command]
fn open_logs_dir(state: State<'_, AppState>) -> Result<(), String> {
    let root = state.config.lock().unwrap().root();
    let dir = root.join("logs");
    let _ = std::fs::create_dir_all(&dir);
    open_folder(dir.to_string_lossy().to_string())
}

#[tauri::command]
fn install_local_mod_cmd(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<String, String> {
    let src = PathBuf::from(path.trim());
    if !src.is_file() {
        return Err("файл не найден".to_string());
    }
    let name = src
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    if !name.to_lowercase().ends_with(".jar") {
        return Err("перетащите .jar-файл мода".to_string());
    }
    if src.metadata().map(|m| m.len()).unwrap_or(0) > 512 * 1024 * 1024 {
        return Err("файл слишком большой (> 512 МБ)".to_string());
    }
    let root = state.config.lock().unwrap().root();
    let dest_dir = root.join("minecraft").join("mods");
    std::fs::create_dir_all(&dest_dir).map_err(|e| e.to_string())?;
    let dest = dest_dir.join(&name);
    std::fs::copy(&src, &dest).map_err(|e| e.to_string())?;
    crate::logger::emit_console(
        &app,
        "info",
        "mods",
        &format!("Локальный мод установлен: {}", name),
    );
    Ok(name)
}

#[tauri::command]
async fn install_mrpack_cmd(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<String, String> {
    let src = PathBuf::from(path.trim());
    if !src.is_file() {
        return Err("файл модпака не найден".to_string());
    }
    if !src
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("mrpack"))
        .unwrap_or(false)
    {
        return Err("выберите файл модпака .mrpack".to_string());
    }
    let root = state.config.lock().unwrap().root();
    spawn_job(&app, state.inner(), "install-mrpack", move |app, job, job_id| {
        Box::pin(async move { modrinth::install_mrpack(&app, &root, &job, &job_id, &src).await })
    })
}

const WALLPAPER_EXTS: [&str; 6] = ["jpg", "jpeg", "png", "webp", "bmp", "gif"];

fn wallpaper_mime(ext: &str) -> &'static str {
    match ext {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        _ => "image/gif",
    }
}

#[tauri::command]
fn set_custom_wallpaper(state: State<'_, AppState>, path: String) -> Result<Config, String> {
    let src = PathBuf::from(path.trim());
    if !src.is_file() {
        return Err("файл обоев не найден".to_string());
    }
    let meta = std::fs::metadata(&src).map_err(|e| e.to_string())?;
    if meta.len() > 12 * 1024 * 1024 {
        return Err("файл слишком большой (максимум 12 МБ)".to_string());
    }
    let ext = src
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();
    if !WALLPAPER_EXTS.contains(&ext.as_str()) {
        return Err("поддерживаются PNG, JPG, WebP, GIF и BMP".to_string());
    }
    let cfg = state.config.lock().unwrap().clone();
    let dest_dir = cfg.root().join("cache");
    std::fs::create_dir_all(&dest_dir).map_err(|e| e.to_string())?;
    let dest = dest_dir.join(format!("custom-wallpaper.{}", ext));
    std::fs::copy(&src, &dest).map_err(|e| e.to_string())?;
    for e2 in WALLPAPER_EXTS.iter() {
        if *e2 != ext {
            let _ = std::fs::remove_file(dest_dir.join(format!("custom-wallpaper.{}", e2)));
        }
    }
    let mut guard = state.config.lock().unwrap();
    guard.wallpaper = "custom".to_string();
    guard.save()?;
    Ok(guard.clone())
}

#[tauri::command]
fn get_custom_wallpaper(state: State<'_, AppState>) -> Result<Option<String>, String> {
    use base64::Engine as _;
    let cfg = state.config.lock().unwrap().clone();
    let dir = cfg.root().join("cache");
    for ext in WALLPAPER_EXTS.iter() {
        let p = dir.join(format!("custom-wallpaper.{}", ext));
        if p.is_file() {
            let bytes = std::fs::read(&p).map_err(|e| e.to_string())?;
            let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
            return Ok(Some(format!(
                "data:{};base64,{}",
                wallpaper_mime(ext),
                b64
            )));
        }
    }
    Ok(None)
}

pub fn run() {
    let app_state = AppState::new();
    if let Err(e) = app_state.config.lock().unwrap().ensure_layout() {
        eprintln!("layout: {}", e);
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            get_config,
            update_config,
            set_root_path,
            get_root_info,
            open_folder,
            list_versions,
            version_size,
            list_installed_versions,
            install_version_cmd,
            loader_versions,
            install_java_cmd,
            get_java_runtimes,
            set_active_version,
            add_offline_account,
            ms_login_start,
            ms_login_finish_cmd,
            remove_account,
            set_active_account,
            modrinth_search_cmd,
            modrinth_project_cmd,
            modrinth_versions_cmd,
            modrinth_install_cmd,
            list_content_files_cmd,
            delete_content_file_cmd,
            launch_game,
            stop_game_cmd,
            cancel_job,
            set_custom_wallpaper,
            get_custom_wallpaper,
            get_play_stats,
            get_launcher_version,
            check_launcher_update,
            get_system_ram_mb,
            reveal_path,
            open_logs_dir,
            install_local_mod_cmd,
            install_mrpack_cmd
        ])
        .run(tauri::generate_context!())
        .expect("failed to run AstralLauncher");
}
