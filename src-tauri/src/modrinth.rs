use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;

use crate::download::{download_all, DownloadItem};
use crate::http::{client, get_json};
use crate::state::JobState;
use crate::util::{dir_size, ensure_dir};

const API: &str = "https://api.modrinth.com/v2";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub project_id: String,
    #[serde(default)]
    pub project_type: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub downloads: u64,
    #[serde(default)]
    pub icon_url: Option<String>,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub date_created: String,
    #[serde(default)]
    pub follows: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResponse {
    #[serde(default)]
    pub hits: Vec<SearchHit>,
    #[serde(default)]
    pub total_hits: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub project_type: String,
    #[serde(default)]
    pub downloads: u64,
    #[serde(default)]
    pub icon_url: Option<String>,
    #[serde(default)]
    pub game_versions: Vec<String>,
    #[serde(default)]
    pub loaders: Vec<String>,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub license: Option<serde_json::Value>,
    #[serde(default)]
    pub date_created: String,
    // В /project/{slug} Modrinth кладёт идентификатор в "id", а не "project_id"
    #[serde(default, alias = "id")]
    pub project_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModFile {
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub filename: String,
    #[serde(default)]
    pub primary: bool,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub hashes: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dependency {
    #[serde(default)]
    pub version_id: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub dependency_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModVersion {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub project_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub version_number: String,
    #[serde(default)]
    pub game_versions: Vec<String>,
    #[serde(default)]
    pub loaders: Vec<String>,
    #[serde(default)]
    pub date_published: String,
    #[serde(default)]
    pub files: Vec<ModFile>,
    #[serde(default)]
    pub dependencies: Vec<Dependency>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileInfo {
    pub name: String,
    pub size_bytes: u64,
    pub is_dir: bool,
}

pub fn content_dir(root: &PathBuf, project_type: &str) -> Result<PathBuf, String> {
    let base = root.join("minecraft");
    match project_type {
        "mod" => Ok(base.join("mods")),
        "resourcepack" => Ok(base.join("resourcepacks")),
        "shader" => Ok(base.join("shaderpacks")),
        "world" => Ok(base.join("saves")),
        _ => Err(format!("неподдерживаемый тип контента: {}", project_type)),
    }
}

pub async fn search(
    query: &str,
    project_type: Option<&str>,
    mc_version: Option<&str>,
    loader: Option<&str>,
    categories: &[String],
    offset: u32,
) -> Result<SearchResponse, String> {
    let mut facets: Vec<Vec<String>> = Vec::new();
    if let Some(pt) = project_type {
        if !pt.is_empty() {
            facets.push(vec![format!("project_type:{}", pt)]);
        }
    }
    if let Some(mc) = mc_version {
        if !mc.is_empty() {
            facets.push(vec![format!("versions:{}", mc)]);
        }
    }
    if let Some(ld) = loader {
        if !ld.is_empty() && ld != "vanilla" {
            facets.push(vec![format!("categories:{}", ld)]);
        }
    }
    if !categories.is_empty() {
        facets.push(categories.iter().map(|c| format!("categories:{}", c)).collect());
    }
    let mut params: Vec<(String, String)> = vec![
        ("limit".to_string(), "20".to_string()),
        ("offset".to_string(), offset.to_string()),
        ("index".to_string(), "relevance".to_string()),
    ];
    if !query.trim().is_empty() {
        params.push(("query".to_string(), query.trim().to_string()));
    }
    if !facets.is_empty() {
        params.push((
            "facets".to_string(),
            serde_json::to_string(&facets).map_err(|e| e.to_string())?,
        ));
    }
    let url = format!("{}/search", API);
    let resp = client()
        .get(&url)
        .query(&params)
        .send()
        .await
        .map_err(|e| format!("modrinth search: {}", e))?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("modrinth search HTTP {}", status));
    }
    serde_json::from_str(&text).map_err(|e| format!("modrinth search json: {}", e))
}

pub async fn project_details(project_id: &str) -> Result<Project, String> {
    let value = get_json(&format!("{}/project/{}", API, project_id)).await?;
    serde_json::from_value(value).map_err(|e| format!("project json: {}", e))
}

pub async fn project_versions(
    project_id: &str,
    mc_version: Option<&str>,
    loader: Option<&str>,
) -> Result<Vec<ModVersion>, String> {
    let url = format!("{}/project/{}/version", API, project_id);
    let mut params: Vec<(String, String)> = Vec::new();
    if let Some(mc) = mc_version {
        if !mc.is_empty() {
            params.push(("game_versions".to_string(), json!([mc]).to_string()));
        }
    }
    if let Some(ld) = loader {
        if !ld.is_empty() && ld != "vanilla" {
            params.push(("loaders".to_string(), json!([ld]).to_string()));
        }
    }
    let resp = client()
        .get(&url)
        .query(&params)
        .send()
        .await
        .map_err(|e| format!("modrinth versions: {}", e))?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("modrinth versions HTTP {}", status));
    }
    serde_json::from_str(&text).map_err(|e| format!("modrinth versions json: {}", e))
}

pub async fn version_by_id(version_id: &str) -> Result<ModVersion, String> {
    let value = get_json(&format!("{}/version/{}", API, version_id)).await?;
    serde_json::from_value(value).map_err(|e| format!("version json: {}", e))
}

fn primary_file(version: &ModVersion) -> Option<&ModFile> {
    version
        .files
        .iter()
        .find(|f| f.primary)
        .or_else(|| version.files.first())
}

/// Из списка совместимых версий игры берём первый релиз (без снапшотов).
fn derive_mc_version(version: &ModVersion) -> String {
    for gv in &version.game_versions {
        if !gv.contains("snapshot")
            && !gv.contains("-pre")
            && !gv.contains("-rc")
            && !gv.contains('w')
        {
            return gv.clone();
        }
    }
    version
        .game_versions
        .first()
        .cloned()
        .unwrap_or_default()
}

fn file_sha1(file: &ModFile) -> Option<String> {
    file.hashes
        .as_ref()
        .and_then(|h| h.get("sha1"))
        .and_then(|s| s.as_str())
        .map(|s| s.to_string())
}

async fn resolve_version_for_project(
    project_id: &str,
    mc_version: Option<&str>,
    loader: Option<&str>,
) -> Result<ModVersion, String> {
    let versions = project_versions(project_id, mc_version, loader).await?;
    versions
        .first()
        .cloned()
        .ok_or_else(|| "нет совместимых версий для выбранной версии игры и загрузчика".to_string())
}

async fn collect_files(
    version: &ModVersion,
    mc_version: Option<&str>,
    loader: Option<&str>,
    depth: u32,
) -> Result<Vec<(String, ModFile)>, String> {
    let mut out: Vec<(String, ModFile)> = Vec::new();
    let mut seen: Vec<String> = vec![version.id.clone()];
    if let Some(f) = primary_file(version) {
        out.push((version.name.clone(), f.clone()));
    }
    let mut queue: Vec<Dependency> = version
        .dependencies
        .iter()
        .filter(|d| d.dependency_type == "required")
        .cloned()
        .collect();
    while let Some(dep) = queue.pop() {
        if depth > 3 {
            continue;
        }
        let dep_version = if let Some(vid) = &dep.version_id {
            if seen.contains(vid) {
                continue;
            }
            seen.push(vid.clone());
            match version_by_id(vid).await {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("dependency {} failed: {}", vid, e);
                    continue;
                }
            }
        } else if let Some(pid) = &dep.project_id {
            match resolve_version_for_project(pid, mc_version, loader).await {
                Ok(v) => {
                    if seen.contains(&v.id) {
                        continue;
                    }
                    seen.push(v.id.clone());
                    v
                }
                Err(e) => {
                    eprintln!("dependency {} failed: {}", pid, e);
                    continue;
                }
            }
        } else {
            continue;
        };
        if let Some(f) = primary_file(&dep_version) {
            out.push((dep_version.name.clone(), f.clone()));
        }
        queue.extend(
            dep_version
                .dependencies
                .iter()
                .filter(|d| d.dependency_type == "required")
                .cloned(),
        );
    }
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
pub async fn install_content(
    app: &tauri::AppHandle,
    root: &PathBuf,
    job: &Arc<JobState>,
    job_id: &str,
    project_id: &str,
    project_title: &str,
    project_type: &str,
    version_id: Option<&str>,
    mc_version: &str,
    loader: &str,
) -> Result<String, String> {
    let mc_opt = if mc_version.is_empty() {
        None
    } else {
        Some(mc_version)
    };
    let ld = if loader.is_empty() || loader == "vanilla" {
        None
    } else {
        Some(loader)
    };
    let version = match version_id {
        Some(vid) if !vid.is_empty() => version_by_id(vid).await?,
        _ => resolve_version_for_project(project_id, mc_opt, ld).await?,
    };
    // Если версия игры не выбрана — выводим её из самой сборки мода.
    let mc: Option<String> = match mc_opt {
        Some(m) => Some(m.to_string()),
        None => {
            let derived = derive_mc_version(&version);
            if derived.is_empty() {
                None
            } else {
                Some(derived)
            }
        }
    };
    let dest_dir = content_dir(root, project_type)?;
    ensure_dir(&dest_dir)?;
    let files = collect_files(&version, mc.as_deref(), ld, 0).await?;
    let mut items: Vec<DownloadItem> = Vec::new();
    let mut installed_names: Vec<String> = Vec::new();
    for (name, file) in &files {
        if file.url.is_empty() {
            continue;
        }
        let dest = dest_dir.join(&file.filename);
        installed_names.push(file.filename.clone());
        items.push(DownloadItem {
            url: file.url.clone(),
            dest,
            sha1: file_sha1(file),
            size: Some(file.size),
        });
        crate::logger::emit_console(
            app,
            "info",
            "modrinth",
            &format!("Файл {}: {} ({})", name, file.filename, crate::version::format_bytes(file.size)),
        );
    }
    if items.is_empty() {
        return Err("у версии нет файлов для скачивания".to_string());
    }
    let label = format!(
        "Установка: {} {}",
        project_title,
        version.version_number
    );
    download_all(app, job, job_id, &label, items).await?;

    if project_type == "world" {
        let zip_path = dest_dir.join(&installed_names[0]);
        let folder_name = zip_path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let extract_to = dest_dir.join(&folder_name);
        ensure_dir(&extract_to)?;
        let app2 = app.clone();
        let job_id2 = job_id.to_string();
        let label2 = label.clone();
        let zip2 = zip_path.clone();
        let dest2 = extract_to.clone();
        let result = tokio::task::spawn_blocking(move || {
            crate::util::extract_zip(&zip2, &dest2, None, Some(&|d, t| {
                crate::download::emit_extract_progress(&app2, &job_id2, &label2, d, t)
            }))
        })
        .await
        .map_err(|e| e.to_string())?;
        result?;
        let _ = std::fs::remove_file(&zip_path);
    }

    let mut msg = format!("Установлено: {} ({})", project_title, version.version_number);
    if installed_names.len() > 1 {
        msg = format!("{} + {} зависимостей", msg, installed_names.len() - 1);
    }
    crate::logger::emit_console(app, "info", "modrinth", &msg);
    Ok(msg)
}

pub fn list_content_files(root: &PathBuf, project_type: &str) -> Result<Vec<FileInfo>, String> {
    let dir = content_dir(root, project_type)?;
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            let md = e.metadata().map_err(|e| e.to_string())?;
            out.push(FileInfo {
                name,
                size_bytes: if md.is_dir() {
                    dir_size(&e.path())
                } else {
                    md.len()
                },
                is_dir: md.is_dir(),
            });
        }
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(out)
}

pub fn delete_content_file(root: &PathBuf, project_type: &str, name: &str) -> Result<(), String> {
    let dir = content_dir(root, project_type)?;
    let path = dir.join(name);
    let safe = path.starts_with(&dir) && name.is_ascii() && !name.contains("..") && !name.contains('/') && !name.contains('\\');
    if !safe {
        return Err("некорректное имя файла".to_string());
    }
    if path.is_dir() {
        std::fs::remove_dir_all(&path).map_err(|e| e.to_string())
    } else if path.is_file() {
        std::fs::remove_file(&path).map_err(|e| e.to_string())
    } else {
        Err("файл не найден".to_string())
    }
}
