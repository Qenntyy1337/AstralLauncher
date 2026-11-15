use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;

use crate::http::{client, get_json};
use crate::java;
use crate::state::JobState;

pub async fn fabric_loader_versions() -> Result<Vec<String>, String> {
    let value = get_json("https://meta.fabricmc.net/v2/versions/loader").await?;
    let arr = value.as_array().ok_or("fabric meta: не массив")?;
    let mut out = Vec::new();
    for item in arr {
        if let Some(v) = item
            .get("loader")
            .and_then(|l| l.get("version"))
            .and_then(|v| v.as_str())
        {
            out.push(v.to_string());
        }
    }
    Ok(out)
}

pub async fn fabric_profile(mc: &str, loader: &str) -> Result<Value, String> {
    let url = format!(
        "https://meta.fabricmc.net/v2/versions/loader/{}/{}/profile/json",
        mc, loader
    );
    get_json(&url).await
}

pub async fn quilt_loader_versions() -> Result<Vec<String>, String> {
    let value = get_json("https://meta.quiltmc.org/v3/versions/loader").await?;
    let arr = value.as_array().ok_or("quilt meta: не массив")?;
    let mut out = Vec::new();
    for item in arr {
        if let Some(v) = item
            .get("loader")
            .and_then(|l| l.get("version"))
            .and_then(|v| v.as_str())
        {
            out.push(v.to_string());
        }
    }
    Ok(out)
}

pub async fn quilt_profile(mc: &str, loader: &str) -> Result<Value, String> {
    let url = format!(
        "https://meta.quiltmc.org/v3/versions/loader/{}/{}/profile/json",
        mc, loader
    );
    get_json(&url).await
}

pub async fn forge_promotions() -> Result<Value, String> {
    get_json("https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json").await
}

fn extract_xml_versions(text: &str) -> Vec<String> {
    let re = regex::Regex::new(r"<version>([^<]+)</version>").expect("static regex");
    re.captures_iter(text)
        .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
        .collect()
}

pub async fn forge_versions(mc: &str) -> Result<Vec<String>, String> {
    let prefix = format!("{}-", mc);
    let mut out: Vec<String> = Vec::new();
    let resp = client()
        .get("https://maven.minecraftforge.net/net/minecraftforge/forge/maven-metadata.xml")
        .send()
        .await;
    if let Ok(resp) = resp {
        if let Ok(text) = resp.text().await {
            out = extract_xml_versions(&text)
                .into_iter()
                .filter(|v| v.starts_with(&prefix))
                .collect();
            out.reverse();
        }
    }
    if out.is_empty() {
        if let Ok(promos) = forge_promotions().await {
            for key in [format!("{}-latest", mc), format!("{}-recommended", mc)] {
                if let Some(build) = promos
                    .get("promos")
                    .and_then(|p| p.get(&key))
                    .and_then(|b| b.as_str())
                {
                    let full = format!("{}-{}", mc, build);
                    if !out.contains(&full) {
                        out.push(full);
                    }
                }
            }
        }
    }
    if out.is_empty() {
        return Err(format!("Forge недоступен для {}", mc));
    }
    out.truncate(30);
    Ok(out)
}

fn neoforge_prefix(mc: &str) -> Option<String> {
    match mc {
        "1.16.5" => Some("36.".to_string()),
        "1.17.1" => Some("37.".to_string()),
        "1.18.2" => Some("40.".to_string()),
        "1.19.2" => Some("41.".to_string()),
        "1.19.3" => Some("42.".to_string()),
        "1.19.4" => Some("43.".to_string()),
        "1.20.1" => Some("47.".to_string()),
        _ => {
            let parts: Vec<&str> = mc.split('.').collect();
            if parts.len() >= 2 && parts[0] == "1" {
                let minor: u32 = parts[1].parse().ok()?;
                let patch: u32 = parts
                    .get(2)
                    .and_then(|p| p.parse().ok())
                    .unwrap_or(0);
                if minor == 20 && patch >= 2 {
                    return Some(format!("20.{}.", patch));
                }
                if minor >= 21 {
                    return Some(format!("{}.{}.", minor, patch));
                }
            }
            None
        }
    }
}

pub async fn neoforge_versions(mc: &str) -> Result<Vec<String>, String> {
    let prefix = neoforge_prefix(mc).ok_or_else(|| format!("NeoForge не поддерживает {}", mc))?;
    let value = get_json("https://maven.neoforged.net/api/maven/versions/releases/net/neoforged/neoforge").await?;
    let arr = value
        .get("versions")
        .and_then(|v| v.as_array())
        .ok_or("neoforge maven: нет versions")?;
    let mut out: Vec<String> = arr
        .iter()
        .filter_map(|v| v.as_str())
        .filter(|v| v.starts_with(&prefix))
        .map(|s| s.to_string())
        .collect();
    out.reverse();
    out.truncate(30);
    if out.is_empty() {
        return Err(format!("NeoForge: нет сборок для {}", mc));
    }
    Ok(out)
}

async fn run_installer(
    app: &tauri::AppHandle,
    java_exe: &PathBuf,
    installer_jar: &PathBuf,
    root: &PathBuf,
) -> Result<(), String> {
    use std::process::Command;
    crate::logger::emit_console(
        app,
        "info",
        "loader",
        &format!("Запуск установщика: {} --installClient {}", installer_jar.display(), root.display()),
    );
    let mut cmd = Command::new(java_exe);
    cmd.arg("-jar")
        .arg(installer_jar)
        .arg("--installClient")
        .arg(root)
        .current_dir(root);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    let output = cmd
        .output()
        .map_err(|e| format!("не удалось запустить установщик: {}", e))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    for line in stdout.lines().take(40) {
        if !line.trim().is_empty() {
            crate::logger::emit_console(app, "info", "installer", line);
        }
    }
    for line in stderr.lines().take(40) {
        if !line.trim().is_empty() {
            crate::logger::emit_console(app, "warn", "installer", line);
        }
    }
    if !output.status.success() {
        return Err(format!(
            "установщик завершился с кодом {}",
            output.status.code().unwrap_or(-1)
        ));
    }
    Ok(())
}

fn newest_version_dir(root: &PathBuf, mc: &str) -> Option<String> {
    let versions = root.join("versions");
    let mut best: Option<(std::time::SystemTime, String)> = None;
    if let Ok(entries) = std::fs::read_dir(&versions) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if !name.contains(mc) {
                continue;
            }
            let json = e.path().join(format!("{}.json", name));
            if !json.is_file() {
                continue;
            }
            let t = e.metadata().and_then(|m| m.modified()).ok()?;
            if best.as_ref().map(|(bt, _)| t > *bt).unwrap_or(true) {
                best = Some((t, name));
            }
        }
    }
    best.map(|(_, n)| n)
}

pub async fn install_loader(
    app: &tauri::AppHandle,
    root: &PathBuf,
    job: &Arc<JobState>,
    job_id: &str,
    loader: &str,
    mc: &str,
    loader_version: Option<&str>,
) -> Result<String, String> {
    match loader {
        "fabric" => {
            let lv = match loader_version {
                Some(v) if !v.is_empty() => v.to_string(),
                _ => fabric_loader_versions()
                    .await?
                    .first()
                    .cloned()
                    .ok_or("fabric meta пуст")?,
            };
            crate::logger::emit_console(
                app,
                "info",
                "loader",
                &format!("Fabric {}: профиль для {}", lv, mc),
            );
            let profile = fabric_profile(mc, &lv).await?;
            crate::version::install_version(app, root, job, job_id, mc, Some(profile)).await
        }
        "quilt" => {
            let lv = match loader_version {
                Some(v) if !v.is_empty() => v.to_string(),
                _ => quilt_loader_versions()
                    .await?
                    .first()
                    .cloned()
                    .ok_or("quilt meta пуст")?,
            };
            crate::logger::emit_console(
                app,
                "info",
                "loader",
                &format!("Quilt {}: профиль для {}", lv, mc),
            );
            let profile = quilt_profile(mc, &lv).await?;
            crate::version::install_version(app, root, job, job_id, mc, Some(profile)).await
        }
        "forge" | "neoforge" => {
            let (builds, installer_url): (Vec<String>, Box<dyn Fn(&str) -> String + Send>) = if loader == "forge" {
                (
                    match loader_version {
                        Some(v) if !v.is_empty() => vec![v.to_string()],
                        _ => forge_versions(mc).await?,
                    },
                    Box::new(|full: &str| {
                        format!(
                            "https://maven.minecraftforge.net/net/minecraftforge/forge/{}/forge-{}-installer.jar",
                            full, full
                        )
                    }),
                )
            } else {
                (
                    match loader_version {
                        Some(v) if !v.is_empty() => vec![v.to_string()],
                        _ => neoforge_versions(mc).await?,
                    },
                    Box::new(|build: &str| {
                        format!(
                            "https://maven.neoforged.net/releases/net/neoforged/neoforge/{}/neoforge-{}-installer.jar",
                            build, build
                        )
                    }),
                )
            };
            let full = builds.first().cloned().ok_or("нет сборок")?;
            let url = installer_url(&full);
            crate::logger::emit_console(app, "info", "loader", &format!("{} {}: {}", loader, full, url));

            let major = if mc.starts_with("1.") {
                let parts: Vec<&str> = mc.split('.').collect();
                let minor: u32 = parts.get(1).and_then(|p| p.parse().ok()).unwrap_or(0);
                let patch: u32 = parts
                    .get(2)
                    .and_then(|p| p.split('-').next())
                    .and_then(|p| p.parse().ok())
                    .unwrap_or(0);
                if minor >= 21 || (minor == 20 && patch >= 5) {
                    21
                } else if minor >= 18 || (minor == 17 && patch >= 1) {
                    17
                } else if minor == 17 {
                    16
                } else {
                    8
                }
            } else {
                21
            };
            let cfg = crate::config::Config::load();
            let java_exe = java::find_java(&cfg, root, major)
                .ok_or(format!("JAVA_MISSING:{}", major))?;

            let dest = root.join("cache").join("installers").join(format!(
                "{}-{}-installer.jar",
                loader, full
            ));
            if !dest.exists() {
                crate::logger::emit_console(app, "info", "loader", "Скачивание установщика...");
                crate::download::download_all(
                    app,
                    job,
                    job_id,
                    &format!("Установщик {}", loader),
                    vec![crate::download::DownloadItem {
                        url,
                        dest: dest.clone(),
                        sha1: None,
                        size: None,
                    }],
                )
                .await?;
            }
            run_installer(app, &java_exe, &dest, root).await?;
            let found = newest_version_dir(root, mc)
                .ok_or("установщик не создал версию")?;
            crate::logger::emit_console(app, "info", "loader", &format!("Установлена версия {}", found));
            Ok(found)
        }
        _ => Err(format!("неизвестный мод-лоадер {}", loader)),
    }
}
