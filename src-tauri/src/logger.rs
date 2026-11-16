use serde::Serialize;
use tauri::{Emitter, Manager};

#[derive(Debug, Clone, Serialize)]
pub struct ConsoleEntry {
    pub ts: String,
    pub level: String,
    pub source: String,
    pub message: String,
}

fn timestamp() -> String {
    chrono::Local::now().format("%H:%M:%S%.3f").to_string()
}

// Путь к файлу лога: root/logs/launcher-YYYY-MM-DD.log.
// Конфиг берём через try_lock: если он уже заблокирован вызывающим кодом,
// запись в файл просто пропускается, чтобы не deadlock'ать.
fn log_file_path(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
    let state = app.try_state::<crate::state::AppState>()?;
    let cfg = state.config.try_lock().ok()?;
    let dir = cfg.root().join("logs");
    let _ = std::fs::create_dir_all(&dir);
    let name = chrono::Local::now().format("launcher-%Y-%m-%d.log").to_string();
    Some(dir.join(name))
}

fn append_file_log(app: &tauri::AppHandle, entry: &ConsoleEntry) {
    if let Some(path) = log_file_path(app) {
        let line = format!(
            "[{}] [{}] [{}] {}\n",
            entry.ts, entry.level, entry.source, entry.message
        );
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
            let _ = std::io::Write::write_all(&mut f, line.as_bytes());
        }
    }
}

pub fn emit_console(app: &tauri::AppHandle, level: &str, source: &str, message: &str) {
    let entry = ConsoleEntry {
        ts: timestamp(),
        level: level.to_string(),
        source: source.to_string(),
        message: message.to_string(),
    };
    let _ = app.emit("console-log", &entry);
    append_file_log(app, &entry);
    match level {
        "error" => eprintln!("[{}] [{}] {}", source, level, message),
        _ => println!("[{}] [{}] {}", source, level, message),
    }
}

pub fn emit_console_from_line(app: &tauri::AppHandle, line: &str) {
    let (level, source, message) = parse_game_line(line);
    emit_console(app, &level, &source, &message);
}

pub fn parse_game_line(line: &str) -> (String, String, String) {
    let re = regex::Regex::new(
        r"^\[[0-9:./ -]+\]\s*\[([^/\]]+)/(\w+)\]\s*(?:\(([^)]*)\):\s*)?(.*)$",
    )
    .expect("static regex");
    if let Some(caps) = re.captures(line) {
        let thread = caps.get(1).map(|m| m.as_str().to_string()).unwrap_or_default();
        let raw_level = caps.get(2).map(|m| m.as_str().to_string()).unwrap_or_default();
        let source = caps
            .get(3)
            .map(|m| m.as_str().to_string())
            .unwrap_or(thread);
        let message = caps.get(4).map(|m| m.as_str().to_string()).unwrap_or_default();
        let level = match raw_level.to_uppercase().as_str() {
            "SEVERE" | "ERROR" | "FATAL" => "error",
            "WARN" | "WARNING" => "warn",
            "DEBUG" | "TRACE" => "debug",
            _ => "info",
        };
        return (level.to_string(), source, message);
    }
    let lower = line.to_lowercase();
    let level = if lower.contains("error") || lower.contains("exception") {
        "error"
    } else if lower.contains("warn") {
        "warn"
    } else {
        "info"
    };
    (level.to_string(), "game".to_string(), line.to_string())
}
