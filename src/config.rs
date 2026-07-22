use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ServerConfig {
    pub name: String,
    pub path: PathBuf,
    #[serde(default = "default_max_ram")]
    pub max_ram: u32,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default = "default_is_fabric")]
    pub is_fabric: bool,
    #[serde(default = "default_loader_type")]
    pub loader_type: String,
}

fn default_max_ram() -> u32 {
    8
}

fn default_version() -> String {
    "26.2".to_string()
}

fn default_is_fabric() -> bool {
    true
}

fn default_loader_type() -> String {
    "fabric".to_string()
}

fn default_language() -> String {
    "uk".to_string()
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct AppConfig {
    pub servers: Vec<ServerConfig>,
    pub selected_index: usize,
    #[serde(default)]
    pub curseforge_key: String,
    #[serde(default = "default_language")]
    pub language: String,
}

impl AppConfig {
    pub fn config_path() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/home/zoozie".to_string());
        let mut path = PathBuf::from(home);
        path.push(".config");
        path.push("mc_server_manager");
        path.push("config.json");
        path
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(mut config) = serde_json::from_str::<AppConfig>(&content) {
                    for srv in &mut config.servers {
                        if srv.loader_type == "fabric" && !srv.is_fabric {
                            srv.loader_type = "vanilla".to_string();
                        }
                    }
                    return config;
                }
            }
        }
        
        // Дефолтна конфігурація: автовизначення існуючого сервера
        let home = std::env::var("HOME").unwrap_or_else(|_| "/home/zoozienix".to_string());
        let default_server_path = PathBuf::from(home).join("Documents").join("minecraft_servers").join("26_1_2");
        let mut servers = Vec::new();
        if default_server_path.join("fabric-server-launch.jar").exists() || default_server_path.join("server.properties").exists() {
            servers.push(ServerConfig {
                name: "26_1_2".to_string(),
                path: default_server_path,
                max_ram: 8,
                version: "26.1.2".to_string(),
                is_fabric: true,
                loader_type: "fabric".to_string(),
            });
        }

        AppConfig {
            servers,
            selected_index: 0,
            curseforge_key: String::new(),
            language: "uk".to_string(),
        }
    }

    pub fn save(&self) -> Result<(), std::io::Error> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        fs::write(path, content)?;
        Ok(())
    }
}
