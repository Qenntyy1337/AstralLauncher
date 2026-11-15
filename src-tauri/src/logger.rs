use serde::Serialize;
use tauri::Emitter;

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

pub fn emit_console(app: &tauri::AppHandle, level: &str, source: &str, message: &str) {
    let entry = ConsoleEntry {
        ts: timestamp(),
        level: level.to_string(),
        source: source.to_string(),
        message: message.to_string(),
    };
    let _ = app.emit("console-log", &entry);
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
