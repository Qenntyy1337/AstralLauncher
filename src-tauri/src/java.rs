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

/// Мажорная версия java по выводу `java -version`:
/// `openjdk version "21.0.1"` → 21, старый формат `"1.8.0_401"` → 8
fn java_major_of(exe: &PathBuf) -> Option<u32> {
    let out = std::process::Command::new(exe)
        .arg("-version")
        .stdin(std::process::Stdio::null())
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stderr);
    let idx = text.find("version ")?;
    let rest = text[idx + 8..].trim_start().trim_start_matches('"');
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    let major: u32 = digits.parse().ok()?;
    if major == 1 {
        let after = &rest[digits.len() + 1..];
        let d2: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
        return d2.parse().ok();
    }
    Some(major)
}

/// Поиск java: явный путь из настроек (без проверки — выбор пользователя),
/// затем встроенный runtime лаунчера, затем системные JDK. Системные
/// кандидаты проверяются фактическим `java -version`: без этого первым
/// находился старый JDK (например 11), который не понимает современные
/// опции (--enable-native-access) и JVM падала на старте.
pub fn find_java(cfg: &Config, root: &PathBuf, major: u32) -> Option<PathBuf> {
    if !cfg.java_path.trim().is_empty() {
        let p = PathBuf::from(cfg.java_path.trim());
        for cand in [p.join("java.exe"), p.join("bin").join("java.exe"), p.join("bin").join("javaw.exe")] {
            if cand.is_file() {
                return Some(cand);
            }
        }
    }
    let rt = runtime_java_exe(root, major);
    if rt.is_file() {
        if let Some(m) = java_major_of(&rt) {
            if m >= major {
                return Some(rt);
            }
        }
    }
    let mut dirs: Vec<PathBuf> = Vec::new();
    for home_key in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Ok(home) = std::env::var(home_key) {
            for base in ["Java", "Eclipse Adoptium", "AdoptOpenJDK", "Zulu", "Microsoft"] {
                if let Ok(entries) = std::fs::read_dir(PathBuf::from(&home).join(base)) {
                    for e in entries.flatten() {
                        dirs.push(e.path());
                    }
                }
            }
        }
    }
    if let Ok(path_var) = std::env::var("PATH") {
        for d in path_var.split(';') {
            let p = PathBuf::from(d);
            if p.join("java.exe").is_file() || p.join("javaw.exe").is_file() {
                dirs.push(p);
            }
        }
    }
    let mut found: Vec<(u32, PathBuf)> = Vec::new();
    for d in dirs {
        let exe = d.join("bin").join("java.exe");
        let exe = if exe.is_file() { exe } else { d.join("java.exe") };
        let exe = if exe.is_file() {
            exe
        } else {
            // некоторые JRE на PATH содержат только javaw.exe
            let jw = d.join("bin").join("javaw.exe");
            let jw2 = d.join("javaw.exe");
            if jw.is_file() {
                jw
            } else if jw2.is_file() {
                jw2
            } else {
                continue;
            }
        };
        if let Some(m) = java_major_of(&exe) {
            found.push((m, exe));
        }
    }
    // точное совпадение требуемой версии, затем минимальная подходящая
    found.sort_by(|a, b| a.0.cmp(&b.0));
    if let Some((_, p)) = found.iter().find(|(m, _)| *m == major) {
        return Some(p.clone());
    }
    found
        .iter()
        .find(|(m, _)| *m >= major)
        .map(|(_, p)| p.clone())
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
