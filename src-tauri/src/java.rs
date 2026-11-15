use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;

use crate::config::Config;
use crate::download::{download_all, DownloadItem};
use crate::http::get_json;
use crate::state::JobState;
use crate::util::{ensure_dir, extract_zip};

#[derive(Debug, Clone, Serialize)]
pub struct JavaCandidate {
    pub path: String,
    pub runtime: bool,
}

pub fn runtime_dir(root: &PathBuf, major: u32) -> PathBuf {
    root.join("runtime").join(format!("java-{}", major))
}

pub fn runtime_java_exe(root: &PathBuf, major: u32) -> PathBuf {
    let dir = runtime_dir(root, major);
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for e in entries.flatten() {
            let candidate = e.path().join("bin").join("java.exe");
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    dir.join("bin").join("java.exe")
}

pub fn find_java(cfg: &Config, root: &PathBuf, major: u32) -> Option<PathBuf> {
    if !cfg.java_path.trim().is_empty() {
        let p = PathBuf::from(cfg.java_path.trim());
        if p.join("java.exe").is_file() {
            return Some(p.join("java.exe"));
        }
        if p.join("bin").join("java.exe").is_file() {
            return Some(p.join("bin").join("java.exe"));
        }
        if p.join("bin").join("javaw.exe").is_file() {
            return Some(p.join("bin").join("javaw.exe"));
        }
    }
    let rt = runtime_java_exe(root, major);
    if rt.is_file() {
        return Some(rt);
    }
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(home) = std::env::var("ProgramFiles") {
        for base in ["Java", "Eclipse Adoptium", "AdoptOpenJDK", "Zulu", "Microsoft"] {
            let dir = PathBuf::from(&home).join(base);
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for e in entries.flatten() {
                    candidates.push(e.path().join("bin").join("javaw.exe"));
                    candidates.push(e.path().join("bin").join("java.exe"));
                }
            }
        }
    }
    if let Ok(x86) = std::env::var("ProgramFiles(x86)") {
        let dir = PathBuf::from(&x86).join("Java");
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for e in entries.flatten() {
                candidates.push(e.path().join("bin").join("javaw.exe"));
                candidates.push(e.path().join("bin").join("java.exe"));
            }
        }
    }
    for c in candidates {
        if c.is_file() {
            return Some(c);
        }
    }
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in path_var.split(';') {
            let exe = PathBuf::from(dir).join("javaw.exe");
            if exe.is_file() {
                return Some(exe);
            }
        }
    }
    None
}

pub fn required_major(model: &crate::version::VersionJson) -> u32 {
    if let Some(jv) = model.java() {
        if let Some(major) = jv.major() {
            return major;
        }
    }
    let id = model.id();
    if id.starts_with("1.") {
        let parts: Vec<&str> = id.split('.').collect();
        if parts.len() >= 2 {
            let minor: u32 = parts[1].parse().unwrap_or(0);
            let patch: u32 = parts
                .get(2)
                .and_then(|p| p.split('-').next())
                .and_then(|p| p.parse().ok())
                .unwrap_or(0);
            if minor >= 21 {
                return 21;
            }
            if minor == 20 && patch >= 5 {
                return 21;
            }
            if minor >= 18 {
                return 17;
            }
            if minor == 17 {
                return 16;
            }
        }
        return 8;
    }
    21
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AdoptiumAsset {
    binary: AdoptiumBinary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AdoptiumBinary {
    package: AdoptiumPackage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AdoptiumPackage {
    name: String,
    link: String,
    size: u64,
    #[serde(default)]
    sha1: Option<String>,
    #[serde(default)]
    checksum: Option<String>,
}

pub async fn install_runtime(
    app: &tauri::AppHandle,
    root: &PathBuf,
    job: &Arc<JobState>,
    job_id: &str,
    major: u32,
) -> Result<PathBuf, String> {
    let url = format!(
        "https://api.adoptium.net/v3/assets/latest/{}/hotspot?architecture=x64&image_type=jre&os=windows&vendor=eclipse",
        major
    );
    let assets: Vec<AdoptiumAsset> = serde_json::from_value(get_json(&url).await?)
        .map_err(|e| format!("ответ Adoptium: {}", e))?;
    let asset = assets.first().ok_or("Adoptium не вернул JRE")?;
    let package = &asset.binary.package;
    let dest_dir = runtime_dir(root, major);
    ensure_dir(&dest_dir)?;
    let tmp_zip = root
        .join("cache")
        .join(format!("jre-{}-{}.zip", major, std::process::id()));
    let item = DownloadItem {
        url: package.link.clone(),
        dest: tmp_zip.clone(),
        sha1: package.sha1.clone(),
        size: Some(package.size),
    };
    crate::logger::emit_console(
        app,
        "info",
        "java",
        &format!("Загрузка JRE {}: {} ({})", major, package.name, crate::version::format_bytes(package.size)),
    );
    download_all(app, job, job_id, &format!("Java {} (Temurin JRE)", major), vec![item]).await?;
    let label = format!("Java {} (распаковка)", major);
    let app2 = app.clone();
    let job_id2 = job_id.to_string();
    let label2 = label.clone();
    let zip_for_task = tmp_zip.clone();
    let result = tokio::task::spawn_blocking(move || {
        extract_zip(&zip_for_task, &dest_dir, None, Some(&|d, t| {
            crate::download::emit_extract_progress(&app2, &job_id2, &label2, d, t)
        }))
    })
    .await
    .map_err(|e| e.to_string())?;
    result?;
    let _ = std::fs::remove_file(&tmp_zip);
    let exe = runtime_java_exe(root, major);
    if !exe.is_file() {
        return Err("JRE распакована, но java.exe не найден".to_string());
    }
    crate::logger::emit_console(app, "info", "java", &format!("JRE {} готова: {}", major, exe.display()));
    Ok(exe)
}

pub fn short_version(java_exe: &PathBuf) -> String {
    for ancestor in java_exe.ancestors() {
        if let Some(name) = ancestor.file_name() {
            let name = name.to_string_lossy();
            if let Some(rest) = name.strip_prefix("java-") {
                return rest.to_string();
            }
            if name.starts_with("jdk") || name.starts_with("jre") {
                return name.to_string();
            }
        }
    }
    "custom".to_string()
}

pub fn list_java_candidates(cfg: &Config, root: &PathBuf) -> Vec<JavaCandidate> {    let mut out = Vec::new();
    let rt_root = root.join("runtime");
    if let Ok(entries) = std::fs::read_dir(&rt_root) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if !name.starts_with("java-") {
                continue;
            }
            if let Ok(sub) = std::fs::read_dir(e.path()) {
                for s in sub.flatten() {
                    let exe = s.path().join("bin").join("java.exe");
                    if exe.is_file() {
                        out.push(JavaCandidate {
                            path: exe.to_string_lossy().to_string(),
                            runtime: true,
                        });
                    }
                }
            }
            let direct = e.path().join("bin").join("java.exe");
            if direct.is_file() {
                out.push(JavaCandidate {
                    path: direct.to_string_lossy().to_string(),
                    runtime: true,
                });
            }
        }
    }
    if !cfg.java_path.trim().is_empty() {
        out.push(JavaCandidate {
            path: cfg.java_path.clone(),
            runtime: false,
        });
    }
    if let Ok(java_home) = std::env::var("JAVA_HOME") {
        let exe = PathBuf::from(java_home).join("bin").join("java.exe");
        if exe.is_file() {
            out.push(JavaCandidate {
                path: exe.to_string_lossy().to_string(),
                runtime: false,
            });
        }
    }
    out
}
