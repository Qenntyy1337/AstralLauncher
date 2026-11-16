use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Запись об одном запуске игры: когда, во что, кем и сколько играли
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchRecord {
    pub ts_ms: u64,
    pub version: String,
    pub account: String,
    pub duration_sec: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PlayStats {
    #[serde(default)]
    pub history: Vec<LaunchRecord>,
    #[serde(default)]
    pub per_version_sec: BTreeMap<String, u64>,
    #[serde(default)]
    pub total_sec: u64,
}

const MAX_HISTORY: usize = 200;

fn stats_path(root: &Path) -> PathBuf {
    root.join("launcher-stats.json")
}

pub fn load(root: &Path) -> PlayStats {
    std::fs::read_to_string(stats_path(root))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save(root: &Path, stats: &PlayStats) {
    if let Ok(text) = serde_json::to_string_pretty(stats) {
        let _ = std::fs::write(stats_path(root), text);
    }
}

/// Запуск игры: добавляем запись в историю (длительность уточнится при выходе)
pub fn record_start(root: &Path, version: &str, account: &str, ts_ms: u64) {
    let mut stats = load(root);
    stats.history.insert(
        0,
        LaunchRecord {
            ts_ms,
            version: version.to_string(),
            account: account.to_string(),
            duration_sec: 0,
        },
    );
    if stats.history.len() > MAX_HISTORY {
        stats.history.truncate(MAX_HISTORY);
    }
    save(root, &stats);
}

/// Завершение игры: обновляем длительность самой свежей незавершённой записи
pub fn record_finish(root: &Path, started_at_ms: u64, ended_at_ms: u64) {
    let duration = ended_at_ms.saturating_sub(started_at_ms) / 1000;
    let mut stats = load(root);
    // самая свежая запись с нулевой длительностью — это текущая сессия
    let version = stats
        .history
        .iter_mut()
        .find(|r| r.duration_sec == 0)
        .map(|rec| {
            rec.duration_sec = duration;
            rec.ts_ms = started_at_ms;
            rec.version.clone()
        });
    if let Some(v) = version {
        *stats.per_version_sec.entry(v).or_insert(0) += duration;
        stats.total_sec += duration;
    }
    save(root, &stats);
}

/// Человекочитаемая расшифровка краша по тексту crash-report
fn crash_hint(description: &str, head: &str) -> String {
    let text = format!("{}\n{}", description, head);
    let lower = text.to_lowercase();
    if lower.contains("outofmemory") || lower.contains("out of memory") {
        return "Игра закончила память: увеличьте ОЗУ в настройках лаунчера (или уменьшите, если модов очень много и памяти выделено слишком много под слабую систему).".to_string();
    }
    if lower.contains("unsupported class file") || lower.contains("unsupportedclassversion") {
        return "Версия Java не подходит этой версии игры — лаунчер обычно подбирает её сам; проверьте путь к Java в настройках.".to_string();
    }
    if lower.contains("mixin") || lower.contains("fabricmc") || lower.contains("fabric") {
        return "Ошибка модов Fabric: чаще всего конфликт или несовместимая версия модов. Уберите недавно добавленные моды или обновите Fabric API.".to_string();
    }
    if lower.contains("fml") || lower.contains("forge") || lower.contains("modfile") {
        return "Ошибка модов Forge: конфликт версий или отсутствующая зависимость. Уберите недавно добавленные моды.".to_string();
    }
    if lower.contains("opengl") || lower.contains("glew") || lower.contains("gl_") || lower.contains("render") || lower.contains("vid") {
        return "Проблема с графикой: обновите драйвер видеокарты. Если игра не стартует вовсе — попробуйте сменить разрешение окна в настройках.".to_string();
    }
    if lower.contains("nosuchmethod") || lower.contains("noclassdef") || lower.contains("classnotfound") || lower.contains("incompatibleclass") {
        return "Конфликт библиотек модов: обычно несовместимые версии модов или Fabric API. Удалите/обновите конфликтующий мод.".to_string();
    }
    if lower.contains("assetindex") || lower.contains("assets") {
        return "Повреждены или отсутствуют файлы ассетов. Переустановите версию — недостающие файлы докачаются.".to_string();
    }
    if lower.contains("access denied") || lower.contains("permission") {
        return "Отказ в доступе к файлам игры: проверьте, что каталог установки не защищён (например, антивирусом или правами папки).".to_string();
    }
    "Причина точно не определена — откройте полный отчёт по кнопке ниже, там будет стек ошибки.".to_string()
}

#[derive(Debug, Clone, Serialize)]
pub struct CrashInfo {
    pub file: String,
    pub description: String,
    pub hint: String,
}

/// Ищем самый свежий crash-report в minecraft/crash-reports и достаём краткую причину
pub fn find_crash_info(game_dir: &Path, after_ms: u64) -> Option<CrashInfo> {
    let dir = game_dir.join("crash-reports");
    let entries = std::fs::read_dir(&dir).ok()?;
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    for e in entries.flatten() {
        let p = e.path();
        let name = p.file_name().unwrap_or_default().to_string_lossy().to_string();
        if !name.starts_with("crash-") || !name.ends_with(".txt") {
            continue;
        }
        let md = e.metadata().ok()?;
        let modified = md.modified().ok()?;
        // берём отчёты, появившиеся после запуска сессии (или последний в любом случае)
        let eligible = modified
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64 >= after_ms.saturating_sub(1000))
            .unwrap_or(false);
        if !eligible {
            continue;
        }
        if best.as_ref().map(|(t, _)| modified > *t).unwrap_or(true) {
            best = Some((modified, p));
        }
    }
    let (_, path) = best?;
    let text = std::fs::read_to_string(&path).ok()?;
    let mut description = String::new();
    for line in text.lines().take(15) {
        if let Some(rest) = line.strip_prefix("Description: ") {
            description = rest.trim().to_string();
            break;
        }
        if !description.is_empty() {
            break;
        }
    }
    if description.is_empty() {
        description = text
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("Неизвестная ошибка")
            .trim()
            .to_string();
    }
    let head: String = text.lines().take(40).collect::<Vec<_>>().join("\n");
    let hint = crash_hint(&description, &head);
    Some(CrashInfo {
        file: path.to_string_lossy().to_string(),
        description,
        hint,
    })
}
