use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::AppHandle;
use tauri::Manager;

use crate::download::{download_all, emit_extract_progress, DownloadItem};
use crate::http::get_json;
use crate::state::{AppState, JobState};
use crate::util::{ensure_dir, eval_rules};

const MANIFEST_URL: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const RESOURCES_URL: &str = "https://resources.download.minecraft.net";

/// Как приходит от Mojang (type / releaseTime)
#[derive(Debug, Clone, Deserialize)]
pub struct RawManifestVersion {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub url: String,
    #[serde(rename = "releaseTime")]
    pub release_time: String,
    #[serde(default)]
    pub sha1: Option<String>,
}

/// Как уходит на фронтенд (kind / release_time)
#[derive(Debug, Clone, Serialize)]
pub struct ManifestVersion {
    pub id: String,
    pub kind: String,
    pub url: String,
    pub release_time: String,
    pub sha1: Option<String>,
}

impl From<RawManifestVersion> for ManifestVersion {
    fn from(r: RawManifestVersion) -> Self {
        Self {
            id: r.id,
            kind: r.kind,
            url: r.url,
            release_time: r.release_time,
            sha1: r.sha1,
        }
    }
}

impl ManifestVersion {
    pub fn is_snapshot_id(id: &str) -> bool {
        id.contains("snapshot")
            || id.contains("-pre")
            || id.contains("-rc")
            || (!id.starts_with("1.") && !id.starts_with("26.") && id.contains('w'))
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SizeBreakdown {
    pub client_bytes: u64,
    pub libraries_bytes: u64,
    pub assets_bytes: u64,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstalledVersion {
    pub id: String,
    pub loader: String,
    pub has_jar: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Artifact {
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibDownloads {
    #[serde(default)]
    pub artifact: Option<Artifact>,
    #[serde(default)]
    pub classifiers: Option<std::collections::HashMap<String, Artifact>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Library {
    pub name: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub downloads: Option<LibDownloads>,
    #[serde(default)]
    pub rules: Option<Vec<crate::util::Rule>>,
    #[serde(default)]
    pub natives: Option<std::collections::HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetIndexRef {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub total_size: Option<u64>,
    #[serde(rename = "totalSize", default)]
    pub total_size_camel: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JavaVersion {
    #[serde(default)]
    pub component: Option<String>,
    #[serde(default)]
    pub major_version: Option<u32>,
    #[serde(rename = "majorVersion", default)]
    pub major_version_camel: Option<u32>,
}

impl JavaVersion {
    pub fn major(&self) -> Option<u32> {
        self.major_version.or(self.major_version_camel)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingClient {
    #[serde(default)]
    pub file: Option<Artifact>,
    #[serde(default)]
    pub argument: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingSection {
    #[serde(default)]
    pub client: Option<LoggingClient>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Downloads {
    #[serde(default)]
    pub client: Option<Artifact>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CondArg {
    pub rules: Vec<crate::util::Rule>,
    pub value: ArgValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ArgValue {
    One(String),
    Many(Vec<String>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ArgPart {
    Plain(String),
    Conditional(CondArg),
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Arguments {
    #[serde(default)]
    pub game: Vec<ArgPart>,
    #[serde(default)]
    pub jvm: Vec<ArgPart>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VersionJson {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub main_class: Option<String>,
    #[serde(rename = "mainClass", default)]
    pub main_class_camel: Option<String>,
    #[serde(default)]
    pub inherits_from: Option<String>,
    #[serde(rename = "inheritsFrom", default)]
    pub inherits_from_camel: Option<String>,
    #[serde(default)]
    pub arguments: Option<Arguments>,
    #[serde(default)]
    pub minecraft_arguments: Option<String>,
    #[serde(rename = "minecraftArguments", default)]
    pub minecraft_arguments_camel: Option<String>,
    #[serde(default)]
    pub libraries: Vec<Library>,
    #[serde(default)]
    pub asset_index: Option<AssetIndexRef>,
    #[serde(rename = "assetIndex", default)]
    pub asset_index_camel: Option<AssetIndexRef>,
    #[serde(default)]
    pub assets: Option<String>,
    #[serde(default)]
    pub downloads: Option<Downloads>,
    #[serde(default)]
    pub java_version: Option<JavaVersion>,
    #[serde(rename = "javaVersion", default)]
    pub java_version_camel: Option<JavaVersion>,
    #[serde(default)]
    pub logging: Option<LoggingSection>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(rename = "type", default)]
    pub kind_camel: Option<String>,
}

impl VersionJson {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn id(&self) -> String {
        self.id.clone().unwrap_or_default()
    }

    pub fn main_class(&self) -> Option<String> {
        self.main_class.clone().or(self.main_class_camel.clone())
    }

    pub fn inherits(&self) -> Option<String> {
        self.inherits_from
            .clone()
            .or(self.inherits_from_camel.clone())
    }

    pub fn args(&self) -> Arguments {
        self.arguments.clone().unwrap_or_default()
    }

    pub fn mc_args_str(&self) -> Option<String> {
        self.minecraft_arguments
            .clone()
            .or(self.minecraft_arguments_camel.clone())
    }

    pub fn asset_index(&self) -> Option<AssetIndexRef> {
        self.asset_index.clone().or(self.asset_index_camel.clone())
    }

    pub fn java(&self) -> Option<JavaVersion> {
        self.java_version.clone().or(self.java_version_camel.clone())
    }

    pub fn kind_name(&self) -> String {
        self.kind
            .clone()
            .or(self.kind_camel.clone())
            .unwrap_or_else(|| "release".to_string())
    }

    pub fn client_artifact(&self) -> Option<Artifact> {
        self.downloads.as_ref().and_then(|d| d.client.clone())
    }

    pub fn logging_client(&self) -> Option<LoggingClient> {
        self.logging.as_ref().and_then(|l| l.client.clone())
    }

    pub fn active_features(&self) -> BTreeMap<String, bool> {
        let mut f = BTreeMap::new();
        f.insert("is_demo_user".to_string(), false);
        f.insert(
            "has_custom_resolution".to_string(),
            false,
        );
        f
    }

    pub fn detect_loader(&self) -> String {
        let mc = self.main_class().unwrap_or_default();
        let libs: String = self
            .libraries
            .iter()
            .map(|l| l.name.clone())
            .collect::<Vec<_>>()
            .join(" ");
        if mc.contains("fabric") || libs.contains("fabric-loader") {
            "fabric".to_string()
        } else if mc.contains("quilt") || libs.contains("quilt-loader") {
            "quilt".to_string()
        } else if mc.contains("neoforge") || libs.contains("neoforge") {
            "neoforge".to_string()
        } else if mc.contains("forge") || libs.contains("minecraftforge") {
            "forge".to_string()
        } else {
            "vanilla".to_string()
        }
    }
}

pub async fn fetch_manifest(
    state: &AppState,
) -> Result<Vec<ManifestVersion>, String> {
    {
        let cache = state.manifest_cache.lock().unwrap();
        if let Some((t, list)) = cache.as_ref() {
            if t.elapsed().as_secs() < 600 {
                return Ok(list.clone());
            }
        }
    }
    let value = get_json(MANIFEST_URL).await?;
    let raw: Vec<RawManifestVersion> = serde_json::from_value(
        value.get("versions").cloned().unwrap_or_default(),
    )
    .map_err(|e| e.to_string())?;
    let versions: Vec<ManifestVersion> = raw.into_iter().map(Into::into).collect();
    *state.manifest_cache.lock().unwrap() =
        Some((std::time::Instant::now(), versions.clone()));
    Ok(versions)
}

pub async fn fetch_manifest_cached(
    app: &AppHandle,
) -> Result<Vec<ManifestVersion>, String> {
    let state = app.state::<AppState>();
    fetch_manifest(state.inner()).await
}

pub fn manifest_entry<'a>(
    list: &'a [ManifestVersion],
    id: &str,
) -> Option<&'a ManifestVersion> {
    list.iter().find(|v| v.id == id)
}

pub async fn fetch_version_json(url: &str) -> Result<serde_json::Value, String> {
    get_json(url).await
}

pub fn version_json_path(root: &PathBuf, id: &str) -> PathBuf {
    root.join("versions").join(id).join(format!("{}.json", id))
}

pub fn client_jar_path(root: &PathBuf, id: &str) -> PathBuf {
    root.join("versions").join(id).join(format!("{}.jar", id))
}

pub fn maven_path_from_name(name: &str) -> String {
    let parts: Vec<&str> = name.split(':').collect();
    if parts.len() < 3 {
        return String::new();
    }
    let group = parts[0].replace('.', "/");
    let artifact = parts[1];
    let version = parts[2];
    let rest = if parts.len() > 3 {
        format!("-{}", parts[3])
    } else {
        String::new()
    };
    format!(
        "{}/{}/{}/{}-{}{}.jar",
        group, artifact, version, artifact, version, rest
    )
}

pub async fn read_version_model(root: &PathBuf, id: &str) -> Result<VersionJson, String> {
    let path = version_json_path(root, id);
    let text = std::fs::read_to_string(&path)
        .map_err(|_| format!("версия {} не установлена", id))?;
    serde_json::from_str::<VersionJson>(&text).map_err(|e| format!("битый JSON {}: {}", id, e))
}

pub async fn read_effective(root: &PathBuf, id: &str) -> Result<VersionJson, String> {
    let mut current = read_version_model(root, id).await?;
    for _ in 0..5 {
        let Some(parent_id) = current.inherits() else {
            break;
        };
        let parent = read_version_model(root, &parent_id).await?;
        let mut merged = current.clone();
        let child = current;
        merged.main_class = child.main_class().or_else(|| parent.main_class().clone());
        merged.main_class_camel = None;
        merged.assets = child.assets.clone().or(parent.assets.clone());
        merged.asset_index = child
            .asset_index()
            .or_else(|| parent.asset_index().clone());
        merged.asset_index_camel = None;
        merged.downloads = child.downloads.clone().or(parent.downloads.clone());
        merged.java_version = child.java().or_else(|| parent.java().clone());
        merged.java_version_camel = None;
        merged.logging = child.logging.clone().or(parent.logging.clone());
        merged.minecraft_arguments = child
            .mc_args_str()
            .or_else(|| parent.mc_args_str().clone());
        merged.minecraft_arguments_camel = None;
        merged.kind = child.kind.clone().or(parent.kind.clone());
        let child_args = child.args();
        let mut parent_args = parent.args();
        parent_args.game.extend(child_args.game);
        parent_args.jvm.extend(child_args.jvm);
        merged.arguments = Some(parent_args);
        let mut libs = parent.libraries.clone();
        libs.extend(child.libraries.clone());
        merged.libraries = libs;
        current = merged;
    }
    Ok(current)
}

fn library_artifact_url(lib: &Library) -> Option<(String, String, Option<String>, Option<u64>)> {
    if let Some(dl) = &lib.downloads {
        if let Some(a) = &dl.artifact {
            let path = a
                .path
                .clone()
                .unwrap_or_else(|| maven_path_from_name(&lib.name));
            let url = a.url.clone()?;
            return Some((path, url, a.sha1.clone(), a.size));
        }
    }
    let path = maven_path_from_name(&lib.name);
    if path.is_empty() {
        return None;
    }
    let base = lib.url.clone()?;
    let url = format!("{}{}", if base.ends_with('/') { base } else { format!("{}/", base) }, path);
    Some((path, url, None, None))
}

pub async fn size_from_model(model: &VersionJson) -> SizeBreakdown {
    let mut client_bytes = 0u64;
    let mut libraries_bytes = 0u64;
    let mut assets_bytes = 0u64;
    if let Some(c) = model.client_artifact() {
        client_bytes += c.size.unwrap_or(0);
    }
    let features = model.active_features();
    for lib in &model.libraries {
        if !eval_rules(&lib.rules, &features) {
            continue;
        }
        if let Some(dl) = &lib.downloads {
            if let Some(a) = &dl.artifact {
                libraries_bytes += a.size.unwrap_or(0);
            }
            if let Some(classifiers) = &dl.classifiers {
                if let Some(n) = classifiers.get("natives-windows") {
                    libraries_bytes += n.size.unwrap_or(0);
                }
            }
        }
    }
    if let Some(li) = model.logging_client().and_then(|l| l.file) {
        libraries_bytes += li.size.unwrap_or(0);
    }
    if let Some(ai) = model.asset_index() {
        assets_bytes += ai.total_size.or(ai.total_size_camel).unwrap_or(0);
    }
    SizeBreakdown {
        client_bytes,
        libraries_bytes,
        assets_bytes,
        total_bytes: client_bytes + libraries_bytes + assets_bytes,
    }
}

/// Размер версии до установки: локальный JSON, если есть, иначе качаем JSON с сервера Mojang.
/// Результат кэшируется в AppState, чтобы не ходить в сеть повторно.
pub async fn version_meta_size(
    state: &AppState,
    id: &str,
) -> Result<SizeBreakdown, String> {
    {
        let cache = state.size_cache.lock().unwrap();
        if let Some(s) = cache.get(id) {
            return Ok(s.clone());
        }
    }
    let root = state.config.lock().unwrap().root();
    let model = match read_version_model(&root, id).await {
        Ok(m) => m,
        Err(_) => {
            let manifest = fetch_manifest(state).await?;
            let entry =
                manifest_entry(&manifest, id).ok_or_else(|| format!("версия {} не найдена в манифесте", id))?;
            let value = fetch_version_json(&entry.url).await?;
            serde_json::from_value::<VersionJson>(value)
                .map_err(|e| format!("битый JSON {}: {}", id, e))?
        }
    };
    let breakdown = size_from_model(&model).await;
    state
        .size_cache
        .lock()
        .unwrap()
        .insert(id.to_string(), breakdown.clone());
    Ok(breakdown)
}

#[allow(clippy::too_many_arguments)]
pub async fn install_version(
    app: &tauri::AppHandle,
    root: &PathBuf,
    job: &Arc<JobState>,
    job_id: &str,
    version_id: &str,
    raw_loader_json: Option<serde_json::Value>,
) -> Result<String, String> {
    let label = format!("Установка {}", version_id);
    let mut base_json_value: Option<serde_json::Value> = None;
    let mut base: VersionJson = VersionJson::empty();
    let mut loader_id = version_id.to_string();
    let mut loader_json: Option<VersionJson> = None;

    if let Some(raw) = &raw_loader_json {
        let parsed: VersionJson = serde_json::from_value(raw.clone())
            .map_err(|e| format!("битый JSON мод-лоадера: {}", e))?;
        loader_id = parsed.id().clone();
        loader_json = Some(parsed.clone());
        if let Some(parent_id) = parsed.inherits() {
            let manifest = crate::version::fetch_manifest_cached(app).await;
            let manifest = manifest?;
            let entry = manifest_entry(&manifest, &parent_id)
                .ok_or_else(|| format!("версия {} не найдена в манифесте", parent_id))?
                .clone();
            base_json_value = Some(fetch_version_json(&entry.url).await?);
            base = serde_json::from_value(base_json_value.clone().unwrap())
                .map_err(|e| format!("битый JSON Mojang: {}", e))?;
        }
    } else {
        let manifest = crate::version::fetch_manifest_cached(app).await?;
        let entry = manifest_entry(&manifest, version_id)
            .ok_or_else(|| format!("версия {} не найдена в манифесте", version_id))?
            .clone();
        base_json_value = Some(fetch_version_json(&entry.url).await?);
        base = serde_json::from_value(base_json_value.clone().unwrap())
            .map_err(|e| format!("битый JSON Mojang: {}", e))?;
    }

    let effective = merge_models(&loader_json, &base, version_id);
    let features = effective.active_features();

    let mut items: Vec<DownloadItem> = Vec::new();
    let mut natives_jars: Vec<PathBuf> = Vec::new();

    if let Some(client) = effective.client_artifact() {
        if let Some(url) = client.url {
            // jar кладём как versions/<id>/<id>.jar: именно этот путь ждут
            // list_installed и запуск (для загрузчика dir = fabric-loader-... и т.п.)
            let jar_dir = loader_id.clone();
            let dest = root
                .join("versions")
                .join(&jar_dir)
                .join(format!("{}.jar", jar_dir));
            items.push(DownloadItem {
                url,
                dest,
                sha1: client.sha1,
                size: client.size,
            });
        }
    }

    for lib in &effective.libraries {
        if !eval_rules(&lib.rules, &features) {
            continue;
        }
        if let Some((path, url, sha1, size)) = library_artifact_url(lib) {
            items.push(DownloadItem {
                url,
                dest: root.join("libraries").join(&path),
                sha1,
                size,
            });
        }
        if let Some(natives) = &lib.natives {
            if let Some(key) = natives.get("natives-windows") {
                if let Some(dl) = &lib.downloads {
                    if let Some(classifiers) = &dl.classifiers {
                        if let Some(nat) = classifiers.get(key) {
                            if let Some(url) = &nat.url {
                                let path = nat.path.clone().unwrap_or_else(|| {
                                    maven_path_from_name(&format!(
                                        "{}:{}:{}:{}",
                                        lib.name.split(':').next().unwrap_or(""),
                                        lib.name.split(':').nth(1).unwrap_or(""),
                                        lib.name.split(':').nth(2).unwrap_or(""),
                                        "natives-windows"
                                    ))
                                });
                                let dest = root.join("libraries").join(&path);
                                natives_jars.push(dest.clone());
                                items.push(DownloadItem {
                                    url: url.clone(),
                                    dest,
                                    sha1: nat.sha1.clone(),
                                    size: nat.size,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    if let Some(lc) = effective.logging_client() {
        if let Some(f) = lc.file {
            if let Some(url) = &f.url {
                let path = f
                    .path
                    .clone()
                    .unwrap_or_else(|| format!("client-{}.xml", version_id));
                items.push(DownloadItem {
                    url: url.clone(),
                    dest: root.join("assets").join("log_configs").join(path),
                    sha1: f.sha1.clone(),
                    size: f.size,
                });
            }
        }
    }

    let mut asset_objects: BTreeMap<String, (String, u64)> = BTreeMap::new();
    if let Some(ai) = effective.asset_index() {
        let aid = ai.id.clone().unwrap_or_else(|| "legacy".to_string());
        if let Some(url) = &ai.url {
            let index_dest = root
                .join("assets")
                .join("indexes")
                .join(format!("{}.json", aid));
            items.push(DownloadItem {
                url: url.clone(),
                dest: index_dest.clone(),
                sha1: ai.sha1.clone(),
                size: None,
            });
            if !index_dest.exists() {
                let idx_value = get_json(url).await?;
                let idx_text = serde_json::to_string(&idx_value).map_err(|e| e.to_string())?;
                ensure_dir(index_dest.parent().unwrap())?;
                std::fs::write(&index_dest, idx_text).map_err(|e| e.to_string())?;
            }
            if let Ok(text) = std::fs::read_to_string(&index_dest) {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                    if let Some(objects) = v.get("objects").and_then(|o| o.as_object()) {
                        for (_name, obj) in objects {
                            if let Some(hash) = obj.get("hash").and_then(|h| h.as_str()) {
                                let size =
                                    obj.get("size").and_then(|s| s.as_u64()).unwrap_or(0);
                                asset_objects.insert(hash.to_string(), (hash.to_string(), size));
                            }
                        }
                    }
                }
            }
        }
    }

    for (hash, (_name, size)) in &asset_objects {
        let prefix = &hash[0..2];
        items.push(DownloadItem {
            url: format!("{}/{}/{}", RESOURCES_URL, prefix, hash),
            dest: root.join("assets").join("objects").join(prefix).join(hash),
            sha1: Some(hash.clone()),
            size: Some(*size),
        });
    }

    crate::logger::emit_console(
        app,
        "info",
        "install",
        &format!(
            "{}: файлов {}, общий объём {}",
            label,
            items.len(),
            format_bytes(items.iter().map(|i| i.size.unwrap_or(0)).sum::<u64>())
        ),
    );

    download_all(app, job, job_id, &label, items).await?;

    let save_json = raw_loader_json
        .clone()
        .or(base_json_value.clone())
        .ok_or("нет JSON версии для сохранения")?;
    let json_dest = version_json_path(root, &loader_id);
    ensure_dir(json_dest.parent().unwrap())?;
    let json_text = serde_json::to_string_pretty(&save_json).map_err(|e| e.to_string())?;
    std::fs::write(&json_dest, json_text).map_err(|e| e.to_string())?;

    let natives_dir = root.join("versions").join(&loader_id).join("natives");
    let natives_list: Vec<PathBuf> = natives_jars.clone();
    for jar in natives_list {
        if !jar.exists() {
            continue;
        }
        let app2 = app.clone();
        let job_id2 = job_id.to_string();
        let label2 = label.clone();
        let natives_dir2 = natives_dir.clone();
        let filter = |name: &str| name.ends_with(".dll") && !name.starts_with("META-INF");
        let result = tokio::task::spawn_blocking(move || {
            crate::util::extract_zip(&jar, &natives_dir2, Some(&filter), Some(&|d, t| {
                emit_extract_progress(&app2, &job_id2, &label2, d, t)
            }))
        })
        .await
        .map_err(|e| e.to_string())?;
        let _ = result;
    }

    crate::logger::emit_console(app, "info", "install", &format!("{}: готово", label));
    Ok(loader_id)
}

fn merge_models(
    loader: &Option<VersionJson>,
    base: &VersionJson,
    version_id: &str,
) -> VersionJson {
    let Some(l) = loader else {
        return base.clone();
    };
    let mut m = base.clone();
    m.id = Some(l.id().clone());
    m.main_class = l.main_class().or_else(|| base.main_class());
    m.main_class_camel = None;
    m.libraries = base
        .libraries
        .iter()
        .chain(l.libraries.iter())
        .cloned()
        .collect();
    let mut args = base.args();
    let largs = l.args();
    args.jvm.extend(largs.jvm);
    args.game.extend(largs.game);
    m.arguments = Some(args);
    m.minecraft_arguments = l.mc_args_str().or_else(|| base.mc_args_str());
    m
        .id
        .get_or_insert_with(|| version_id.to_string());
    m
}

pub fn native_jar_paths(model: &VersionJson, root: &PathBuf) -> Vec<PathBuf> {
    let features = model.active_features();
    let mut out = Vec::new();
    for lib in &model.libraries {
        if !eval_rules(&lib.rules, &features) {
            continue;
        }
        let has_natives = lib
            .natives
            .as_ref()
            .map(|n| n.contains_key("natives-windows"))
            .unwrap_or(false);
        if !has_natives {
            continue;
        }
        if let Some(dl) = &lib.downloads {
            if let Some(classifiers) = &dl.classifiers {
                if let Some(nat) = classifiers.get("natives-windows") {
                    let path = nat.path.clone().unwrap_or_else(|| {
                        maven_path_from_name(&format!(
                            "{}:{}:{}:{}",
                            lib.name.split(':').next().unwrap_or(""),
                            lib.name.split(':').nth(1).unwrap_or(""),
                            lib.name.split(':').nth(2).unwrap_or(""),
                            "natives-windows"
                        ))
                    });
                    let p = root.join("libraries").join(&path);
                    if p.is_file() {
                        out.push(p);
                    }
                }
            }
        }
    }
    out
}

pub fn list_installed(root: &PathBuf) -> Vec<InstalledVersion> {
    let mut out = Vec::new();
    let versions_dir = root.join("versions");
    let Ok(entries) = std::fs::read_dir(&versions_dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let dir_name = entry.file_name().to_string_lossy().to_string();
        let json_path = versions_dir.join(&dir_name).join(format!("{}.json", dir_name));
        if !json_path.is_file() {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&json_path) else {
            continue;
        };
        let Ok(model) = serde_json::from_str::<VersionJson>(&text) else {
            continue;
        };
        let has_jar = client_jar_path(&versions_dir.parent().unwrap().to_path_buf(), &dir_name)
            .exists()
            || entry
                .path()
                .join(format!("{}.jar", dir_name))
                .exists();
        out.push(InstalledVersion {
            id: dir_name,
            loader: model.detect_loader(),
            has_jar,
        });
    }
    out.sort_by(|a, b| b.id.cmp(&a.id));
    out
}

pub fn format_bytes(n: u64) -> String {
    let f = n as f64;
    if f >= 1024.0 * 1024.0 * 1024.0 {
        format!("{:.2} ГБ", f / (1024.0 * 1024.0 * 1024.0))
    } else if f >= 1024.0 * 1024.0 {
        format!("{:.1} МБ", f / (1024.0 * 1024.0))
    } else if f >= 1024.0 {
        format!("{:.0} КБ", f / 1024.0)
    } else {
        format!("{} Б", n)
    }
}
