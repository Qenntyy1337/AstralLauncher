use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsRule {
    pub name: Option<String>,
    pub arch: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub action: String,
    #[serde(default)]
    pub os: Option<OsRule>,
    #[serde(default)]
    pub features: Option<std::collections::BTreeMap<String, bool>>,
}

fn os_matches(os: &Option<OsRule>) -> bool {
    match os {
        None => true,
        Some(o) => match &o.name {
            Some(n) => n == "windows",
            None => true,
        },
    }
}

fn features_match(rules: &Option<std::collections::BTreeMap<String, bool>>, active: &std::collections::BTreeMap<String, bool>) -> bool {
    match rules {
        None => true,
        Some(map) => map.iter().all(|(k, want)| active.get(k) == Some(want)),
    }
}

pub fn rule_applies(rule: &Rule, active_features: &std::collections::BTreeMap<String, bool>) -> bool {
    os_matches(&rule.os) && features_match(&rule.features, active_features)
}

pub fn eval_rules(rules: &Option<Vec<Rule>>, active_features: &std::collections::BTreeMap<String, bool>) -> bool {
    match rules {
        None => true,
        Some(list) => {
            let mut allowed = false;
            for r in list {
                if rule_applies(r, active_features) {
                    allowed = r.action == "allow";
                }
            }
            allowed
        }
    }
}

pub fn sha1_file(path: &Path) -> Result<String, String> {
    let mut f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha1::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = f.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

pub fn ensure_dir(path: &Path) -> Result<(), String> {
    std::fs::create_dir_all(path).map_err(|e| format!("{}: {}", path.display(), e))
}

pub fn extract_zip(
    zip_path: &Path,
    dest_dir: &Path,
    filter: Option<&dyn Fn(&str) -> bool>,
    progress: Option<&dyn Fn(u64, u64)>,
) -> Result<Vec<String>, String> {
    let file = std::fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let total = archive.len() as u64;
    let mut extracted = Vec::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().to_string();
        if name.ends_with('/') {
            if let Some(p) = progress {
                p(i as u64, total);
            }
            continue;
        }
        if let Some(f) = filter {
            if !f(&name) {
                if let Some(p) = progress {
                    p(i as u64, total);
                }
                continue;
            }
        }
        let Some(rel) = entry.enclosed_name() else {
            if let Some(p) = progress {
                p(i as u64, total);
            }
            continue;
        };
        let out_path = dest_dir.join(rel);
        if let Some(parent) = out_path.parent() {
            ensure_dir(parent)?;
        }
        let mut out = std::fs::File::create(&out_path).map_err(|e| e.to_string())?;
        std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
        extracted.push(name);
        if let Some(p) = progress {
            p(i as u64, total);
        }
    }
    Ok(extracted)
}

pub fn now_millis() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn unique_id(prefix: &str) -> String {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("{}-{}-{}", prefix, now_millis(), n)
}

pub fn dir_size(path: &Path) -> u64 {
    let mut total = 0u64;
    if let Ok(rd) = std::fs::read_dir(path) {
        for e in rd.flatten() {
            if let Ok(md) = e.metadata() {
                if md.is_file() {
                    total += md.len();
                } else if md.is_dir() {
                    total += dir_size(&e.path());
                }
            }
        }
    }
    total
}

#[allow(dead_code)]
pub fn normalize_path(p: &str) -> PathBuf {
    PathBuf::from(p.trim().trim_matches('"'))
}
