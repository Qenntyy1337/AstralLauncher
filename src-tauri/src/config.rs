use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const DEFAULT_ROOT: &str = "C:\\AstralLauncher";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    pub kind: String,
    pub username: String,
    pub uuid: String,
    #[serde(default)]
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: String,
    #[serde(default)]
    pub expires_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub root_dir: String,
    pub ram_mb: u32,
    pub java_path: String,
    pub extra_jvm_args: Vec<String>,
    pub extra_game_args: Vec<String>,
    pub width: u32,
    pub height: u32,
    pub keep_launcher_open: bool,
    pub ms_client_id: String,
    pub accounts: Vec<Account>,
    pub active_account_id: Option<String>,
    pub active_version: Option<String>,
    pub wallpaper: String,
    pub wallpaper_blur: u32,
    pub wallpaper_dim: u32,
    pub play_loader: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            root_dir: DEFAULT_ROOT.to_string(),
            ram_mb: 4096,
            java_path: String::new(),
            extra_jvm_args: Vec::new(),
            extra_game_args: Vec::new(),
            width: 0,
            height: 0,
            keep_launcher_open: true,
            ms_client_id: String::new(),
            accounts: Vec::new(),
            active_account_id: None,
            active_version: None,
            wallpaper: "aurora".to_string(),
            wallpaper_blur: 18,
            wallpaper_dim: 40,
            play_loader: "vanilla".to_string(),
        }
    }
}

impl Config {
    pub fn root(&self) -> PathBuf {
        PathBuf::from(&self.root_dir)
    }

    pub fn config_path(root: &PathBuf) -> PathBuf {
        root.join("launcher-config.json")
    }

    pub fn load() -> Self {
        let root = PathBuf::from(DEFAULT_ROOT);
        let path = Self::config_path(&root);
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Ok(mut cfg) = serde_json::from_str::<Config>(&text) {
                if cfg.root_dir.trim().is_empty() {
                    cfg.root_dir = DEFAULT_ROOT.to_string();
                }
                return cfg;
            }
        }
        let cfg = Config::default();
        let _ = std::fs::create_dir_all(&root);
        let _ = cfg.save();
        cfg
    }

    pub fn save(&self) -> Result<(), String> {
        let root = self.root();
        std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(Self::config_path(&root), text).map_err(|e| e.to_string())
    }

    pub fn ensure_layout(&self) -> Result<(), String> {
        let root = self.root();
        for sub in [
            "versions",
            "libraries",
            "assets",
            "runtime",
            "cache",
            "minecraft",
            "minecraft/mods",
            "minecraft/resourcepacks",
            "minecraft/shaderpacks",
            "minecraft/saves",
            "minecraft/screenshots",
            "minecraft/logs",
            "cache/installers",
            "minecraft/log_configs",
        ] {
            crate::util::ensure_dir(&root.join(sub))?;
        }
        Ok(())
    }

    pub fn account_by_id(&self, id: &str) -> Option<&Account> {
        self.accounts.iter().find(|a| a.id == id)
    }

    pub fn active_account(&self) -> Option<&Account> {
        match &self.active_account_id {
            Some(id) => self.account_by_id(id),
            None => self.accounts.first(),
        }
    }
}
