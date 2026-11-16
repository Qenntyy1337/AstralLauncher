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
        let v = item
            .get("version")
            .and_then(|v| v.as_str())
            .or_else(|| {
                item.get("loader")
                    .and_then(|l| l.get("version"))
                    .and_then(|v| v.as_str())
            });
        if let Some(v) = v {
            out.push(v.to_string());
        }
    }
    if out.is_empty() {
        return Err("fabric meta пуст".to_string());
    }
    Ok(out)
}

pub async fn fabric_profile(mc: &str, loader: &str) -> Result<Value, String> {
    let url = format!(
        "https://meta.fabricmc.net/v2/versions/loader/{}/{}/profile/json",
        mc, loader
    );
    let value = get_json(&url).await?;
    if value.get("inheritsFrom").is_none() && value.get("mainClass").is_none() {
        return Err(format!("Fabric: нет профиля для {} (мета вернула ошибку)", mc));
    }
    Ok(value)
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
    if crate::version::ManifestVersion::is_snapshot_id(mc) {
        return Err(format!(
            "Forge не поддерживает снапшоты: {}. Выберите релиз, например 26.3",
            mc
        ));
    }
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

async fn required_java_for(app: &tauri::AppHandle, mc: &str) -> Result<u32, String> {
    let manifest = crate::version::fetch_manifest_cached(app).await?;
    let entry = crate::version::manifest_entry(&manifest, mc)
        .ok_or_else(|| format!("версия {} не найдена в манифесте", mc))?;
    let value = crate::version::fetch_version_json(&entry.url).await?;
    let model: crate::version::VersionJson =
        serde_json::from_value(value).map_err(|e| format!("битый JSON {}: {}", mc, e))?;
    Ok(java::required_major(&model))
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
        "forge" => {
            if crate::version::ManifestVersion::is_snapshot_id(mc) {
                return Err(format!(
                    "Forge не поддерживает снапшоты: {}. Выберите релиз, например 26.3",
                    mc
                ));
            }
            let full = match loader_version {
                Some(v) if !v.is_empty() => v.to_string(),
                _ => forge_versions(mc)
                    .await?
                    .first()
                    .cloned()
                    .ok_or("нет сборок Forge")?,
            };
            let url = format!(
                "https://maven.minecraftforge.net/net/minecraftforge/forge/{}/forge-{}-installer.jar",
                full, full
            );
            crate::logger::emit_console(app, "info", "loader", &format!("Forge {}: {}", full, url));

            let major = required_java_for(app, mc).await.unwrap_or(21);
            let cfg = crate::config::Config::load();
            let java_exe = java::find_java(&cfg, root, major)
                .ok_or(format!("JAVA_MISSING:{}", major))?;

            let dest = root
                .join("cache")
                .join("installers")
                .join(format!("forge-{}-installer.jar", full));
            if !dest.exists() {
                crate::logger::emit_console(app, "info", "loader", "Скачивание установщика...");
                crate::download::download_all(
                    app,
                    job,
                    job_id,
                    "Установщик Forge",
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
