use serde::Serialize;
use std::collections::BTreeMap;
use std::path::PathBuf;
use tauri::Emitter;
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::config::{Account, Config};
use crate::state::AppState;
use crate::util::{ensure_dir, eval_rules};
use crate::version::{self, VersionJson};

#[derive(Debug, Clone, Serialize)]
pub struct LaunchInfo {
    pub pid: u32,
    pub version: String,
    pub java: String,
    pub memory_mb: u32,
}

fn expand_placeholders(
    arg: &str,
    map: &BTreeMap<String, String>,
) -> String {
    let mut result = arg.to_string();
    while let Some(start) = result.find("${") {
        let Some(end_rel) = result[start..].find('}') else {
            break;
        };
        let end = start + end_rel;
        let key = &result[start + 2..end];
        let value = map.get(key).cloned().unwrap_or_default();
        result = format!("{}{}{}", &result[..start], value, &result[end + 1..]);
    }
    result
}

fn expand_args(
    parts: &[version::ArgPart],
    features: &BTreeMap<String, bool>,
    map: &BTreeMap<String, String>,
) -> Vec<String> {
    let mut out = Vec::new();
    for part in parts {
        match part {
            version::ArgPart::Plain(s) => out.push(expand_placeholders(s, map)),
            version::ArgPart::Conditional(c) => {
                if eval_rules(&Some(c.rules.clone()), features) {
                    match &c.value {
                        version::ArgValue::One(s) => out.push(expand_placeholders(s, map)),
                        version::ArgValue::Many(list) => {
                            for s in list {
                                out.push(expand_placeholders(s, map));
                            }
                        }
                    }
                }
            }
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
pub async fn launch(
    app: &tauri::AppHandle,
    state: &AppState,
    version_id: &str,
) -> Result<LaunchInfo, String> {
    let cfg: Config = state.config.lock().unwrap().clone();
    let root = cfg.root();

    let Some(account) = cfg.active_account().cloned() else {
        return Err("создайте или выберите аккаунт в Настройках".to_string());
    };
    let account = refresh_account(&app, state, account).await?;

    let model = match version::read_effective(&root, version_id).await {
        Ok(m) => m,
        Err(e) => {
            // самовосстановление: докачиваем недостающие json версии/родителя
            let healed = version::heal_version_chain(app, &root, version_id)
                .await
                .unwrap_or(false);
            if healed {
                version::read_effective(&root, version_id).await?
            } else {
                return Err(e);
            }
        }
    };
    let major = crate::java::required_major(&model);
    let java_exe = crate::java::find_java(&cfg, &root, major)
        .ok_or(format!("JAVA_MISSING:{}", major))?;
    let java_exe = if java_exe
        .parent()
        .map(|p| p.join("javaw.exe").is_file())
        .unwrap_or(false)
    {
        java_exe.parent().unwrap().join("javaw.exe")
    } else {
        java_exe
    };

    let client_jar = version::resolve_client_jar(&root, version_id);
    if !client_jar.is_file() {
        return Err(format!("файл версии {} не найден, установите её заново", version_id));
    }

    let game_dir = root.join("minecraft");
    ensure_dir(&game_dir)?;
    let natives_dir = root.join("versions").join(version_id).join("natives");
    ensure_dir(&natives_dir)?;
    ensure_natives(&model, &root, &natives_dir)?;

    let assets_id = model
        .asset_index()
        .and_then(|ai| ai.id)
        .or_else(|| model.assets.clone())
        .unwrap_or_else(|| "legacy".to_string());

    let mut classpath_parts: Vec<String> = Vec::new();
    let features = model.active_features();
    for lib in &model.libraries {
        if !eval_rules(&lib.rules, &features) {
            continue;
        }
        if let Some(dl) = &lib.downloads {
            if let Some(a) = &dl.artifact {
                if let Some(path) = &a.path {
                    let p = root.join("libraries").join(path);
                    if p.is_file() {
                        classpath_parts.push(p.to_string_lossy().to_string());
                    }
                } else {
                    let path = version::maven_path_from_name(&lib.name);
                    let p = root.join("libraries").join(&path);
                    if p.is_file() {
                        classpath_parts.push(p.to_string_lossy().to_string());
                    }
                }
            } else {
                let path = version::maven_path_from_name(&lib.name);
                if !path.is_empty() {
                    let p = root.join("libraries").join(&path);
                    if p.is_file() {
                        classpath_parts.push(p.to_string_lossy().to_string());
                    }
                }
            }
        }
    }
    classpath_parts.push(client_jar.to_string_lossy().to_string());
    let classpath = classpath_parts.join(";");
    let classpath_separator = ";";

    let log_config_path = model
        .logging_client()
        .and_then(|lc| lc.file)
        .and_then(|f| {
            let p = root
                .join("assets")
                .join("log_configs")
                .join(f.path.unwrap_or_else(|| format!("client-{}.xml", version_id)));
            if p.is_file() {
                Some(p)
            } else {
                None
            }
        });

    let mut map: BTreeMap<String, String> = BTreeMap::new();
    map.insert("natives_directory".into(), natives_dir.to_string_lossy().to_string());
    map.insert("launcher_name".into(), "AstralLauncher".into());
    map.insert("launcher_version".into(), "1.0.0".into());
    map.insert("classpath".into(), classpath.clone());
    map.insert("classpath_separator".into(), classpath_separator.into());
    map.insert("library_directory".into(), root.join("libraries").to_string_lossy().to_string());
    map.insert("assets_root".into(), root.join("assets").to_string_lossy().to_string());
    map.insert("assets_index_name".into(), assets_id.clone());
    map.insert("game_directory".into(), game_dir.to_string_lossy().to_string());
    map.insert("version_name".into(), model.id());
    map.insert("version_type".into(), model.kind_name());
    map.insert("auth_player_name".into(), account.username.clone());
    map.insert("auth_uuid".into(), account.uuid.clone());
    map.insert("auth_access_token".into(), token_for_launch(&account));
    map.insert("auth_xuid".into(), "0".into());
    map.insert("clientid".into(), String::new());
    map.insert("auth_session".into(), token_for_launch(&account));
    map.insert("user_properties".into(), "{}".into());
    map.insert(
        "user_type".into(),
        if account.kind == "microsoft" { "msa".into() } else { "legacy".into() },
    );
    if let Some(p) = &log_config_path {
        map.insert("path".into(), p.to_string_lossy().to_string());
    }

    let mut features = model.active_features();
    let has_resolution = cfg.width > 0 && cfg.height > 0;
    if has_resolution {
        features.insert("has_custom_resolution".into(), true);
        map.insert("resolution_width".into(), cfg.width.to_string());
        map.insert("resolution_height".into(), cfg.height.to_string());
    }

    let mut args: Vec<String> = Vec::new();
    args.push(format!("-Xmx{}M", cfg.ram_mb));
    args.push(format!("-Xms{}M", (cfg.ram_mb / 2).max(512)));
    if let Some(p) = &log_config_path {
        args.push(format!("-Dlog4j.configurationFile={}", p.to_string_lossy()));
    }

    let jvm_args = model.args().jvm;
    let mut expanded_jvm = expand_args(&jvm_args, &features, &map);
    if expanded_jvm.is_empty() {
        expanded_jvm.push(format!("-Djava.library.path={}", natives_dir.to_string_lossy()));
        expanded_jvm.push("-cp".into());
        expanded_jvm.push(classpath.clone());
    }
    args.extend(expanded_jvm);
    for extra in &cfg.extra_jvm_args {
        if !extra.trim().is_empty() {
            args.push(extra.trim().to_string());
        }
    }

    let main_class = model
        .main_class()
        .filter(|m| !m.trim().is_empty())
        .ok_or("в версии не указан mainClass")?;
    args.push(main_class);

    let game_args = model.args().game;
    if game_args.is_empty() {
        let Some(raw) = model.mc_args_str() else {
            return Err("в версии нет аргументов запуска игры".to_string());
        };
        for token in raw.split_whitespace() {
            args.push(expand_placeholders(token, &map));
        }
    } else {
        args.extend(expand_args(&game_args, &features, &map));
    }
    for extra in &cfg.extra_game_args {
        if !extra.trim().is_empty() {
            args.push(extra.trim().to_string());
        }
    }

    crate::logger::emit_console(
        &app,
        "info",
        "launch",
        &format!(
            "Запуск {} | Java {} | ОЗУ {} МБ | аккаунт {} ({})",
            model.id(),
            crate::java::short_version(&java_exe),
            cfg.ram_mb,
            account.username,
            account.kind
        ),
    );

    let mut cmd = tokio::process::Command::new(&java_exe);
    cmd.args(&args)
        .current_dir(&game_dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .stdin(std::process::Stdio::null());
    #[cfg(windows)]
    {
        cmd.creation_flags(0x08000000);
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("не удалось запустить java: {}", e))?;
    let pid = child.id().unwrap_or(0);

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let app_out = app.clone();
    let app_err = app.clone();
    if let Some(out) = stdout {
        tokio::spawn(async move {
            let reader = BufReader::new(out);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                crate::logger::emit_console_from_line(&app_out, &line);
            }
        });
    }
    if let Some(err) = stderr {
        tokio::spawn(async move {
            let reader = BufReader::new(err);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                crate::logger::emit_console(&app_err, "error", "stderr", &line);
            }
        });
    }

    let app_wait = app.clone();
    tokio::spawn(async move {
        let status = child.wait().await;
        let code = status
            .map(|s| s.code().unwrap_or(-1))
            .unwrap_or(-1);
        crate::logger::emit_console(
            &app_wait,
            if code == 0 { "info" } else { "warn" },
            "launch",
            &format!("Процесс игры завершён с кодом {}", code),
        );
        let _ = app_wait.emit(
            "game-exit",
            serde_json::json!({ "pid": pid, "code": code }),
        );
    });

    Ok(LaunchInfo {
        pid,
        version: model.id(),
        java: java_exe.to_string_lossy().to_string(),
        memory_mb: cfg.ram_mb,
    })
}

fn token_for_launch(account: &Account) -> String {
    if account.kind == "microsoft" && !account.access_token.is_empty() {
        account.access_token.clone()
    } else {
        "0".to_string()
    }
}

async fn refresh_account(
    _app: &tauri::AppHandle,
    state: &AppState,
    account: Account,
) -> Result<Account, String> {
    let cfg = state.config.lock().unwrap().clone();
    let updated = crate::accounts::refresh_token(&cfg, &account).await?;
    if updated.access_token != account.access_token {
        let mut cfg_guard = state.config.lock().unwrap();
        if let Some(acc) = cfg_guard
            .accounts
            .iter_mut()
            .find(|a| a.id == account.id)
        {
            *acc = updated.clone();
        }
        let _ = cfg_guard.save();
    }
    Ok(updated)
}

fn ensure_natives(model: &VersionJson, root: &PathBuf, natives_dir: &PathBuf) -> Result<(), String> {
    let has_files = std::fs::read_dir(natives_dir)
        .map(|mut rd| rd.next().is_some())
        .unwrap_or(false);
    if has_files {
        return Ok(());
    }
    let jars = version::native_jar_paths(model, root);
    for jar in jars {
        let filter = |name: &str| name.ends_with(".dll") && !name.starts_with("META-INF");
        crate::util::extract_zip(&jar, natives_dir, Some(&filter), None)?;
    }
    Ok(())
}

pub fn stop_game(pid: u32) -> Result<(), String> {
    if pid == 0 {
        return Err("некорректный PID".to_string());
    }
    let output = std::process::Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .creation_flags_cfg()
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err("процесс не найден или уже завершён".to_string())
    }
}

trait CreationFlagsCfg {
    fn creation_flags_cfg(&mut self) -> &mut Self;
}

#[cfg(windows)]
impl CreationFlagsCfg for std::process::Command {
    fn creation_flags_cfg(&mut self) -> &mut Self {
        use std::os::windows::process::CommandExt;
        self.creation_flags(0x08000000)
    }
}

#[cfg(not(windows))]
impl CreationFlagsCfg for std::process::Command {
    fn creation_flags_cfg(&mut self) -> &mut Self {
        self
    }
}
