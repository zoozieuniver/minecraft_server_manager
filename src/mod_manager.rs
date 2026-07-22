#![allow(dead_code)]
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::io::Read;

#[derive(Deserialize, Debug, Clone)]
pub struct ModrinthSearchHit {
    pub project_id: String,
    pub title: String,
    pub description: String,
    pub client_side: String,
    pub server_side: String,
    pub icon_url: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ModrinthSearchResponse {
    pub hits: Vec<ModrinthSearchHit>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ModrinthVersionFile {
    pub url: String,
    pub filename: String,
    pub primary: bool,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ModrinthVersion {
    pub id: String,
    pub name: String,
    pub version_number: String,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    pub files: Vec<ModrinthVersionFile>,
}

// Modrinth Index JSON for .mrpack
#[derive(Deserialize, Debug, Clone)]
pub struct MrpackIndexEnv {
    pub client: String,
    pub server: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct MrpackIndexFile {
    pub path: String,
    pub env: Option<MrpackIndexEnv>,
    pub downloads: Vec<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct MrpackIndex {
    pub files: Vec<MrpackIndexFile>,
}

// CurseForge API structures
#[derive(Deserialize, Debug, Clone)]
pub struct CurseForgeLogo {
    #[serde(rename = "thumbnailUrl")]
    pub thumbnail_url: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct CurseForgeFile {
    pub id: u64,
    #[serde(rename = "displayName")]
    pub display_name: String,
    #[serde(rename = "fileName")]
    pub file_name: String,
    #[serde(rename = "downloadUrl")]
    pub download_url: Option<String>,
    #[serde(rename = "gameVersions")]
    pub game_versions: Vec<String>,
    #[serde(rename = "modLoaderType")]
    pub mod_loader_type: Option<u32>, // 4 is Fabric
}

#[derive(Deserialize, Debug, Clone)]
pub struct CurseForgeMod {
    pub id: u64,
    pub name: String,
    pub summary: String,
    pub logo: Option<CurseForgeLogo>,
    #[serde(rename = "latestFiles")]
    pub latest_files: Vec<CurseForgeFile>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct CurseForgeSearchResponse {
    pub data: Vec<CurseForgeMod>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct CurseForgeDownloadUrlResponse {
    pub data: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct CurseForgeModResponse {
    pub data: CurseForgeMod,
}

// CurseForge Modpack Manifest structures
#[derive(Deserialize, Debug, Clone)]
pub struct CurseForgeManifestFile {
    #[serde(rename = "projectID")]
    pub project_id: u64,
    #[serde(rename = "fileID")]
    pub file_id: u64,
    pub required: bool,
}

#[derive(Deserialize, Debug, Clone)]
pub struct CurseForgeManifestMinecraftLoader {
    pub id: String,
    pub primary: bool,
}

#[derive(Deserialize, Debug, Clone)]
pub struct CurseForgeManifestMinecraft {
    pub version: String,
    #[serde(rename = "modLoaders")]
    pub mod_loaders: Vec<CurseForgeManifestMinecraftLoader>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct CurseForgeManifest {
    pub minecraft: CurseForgeManifestMinecraft,
    pub files: Vec<CurseForgeManifestFile>,
    pub overrides: String,
}

// Metadata for tracked mods to support auto-updates
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct InstalledModMetadata {
    pub filename: String,
    pub source: String, // "modrinth" or "curseforge"
    pub project_id: String,
    pub version_id: String,
    #[serde(default)]
    pub icon_url: Option<String>,
}

#[derive(Clone, Debug)]
pub struct InstalledModInfo {
    pub filename: String,
    pub icon_url: Option<String>,
}

// Пошук проектів на Modrinth
pub fn search_modrinth_projects(query: &str) -> Result<Vec<ModrinthSearchHit>, String> {
    if query.trim().is_empty() {
        return Ok(Vec::new());
    }
    
    let encoded_query = urlencoding::encode(query);
    let url = format!(
        "https://api.modrinth.com/v2/search?query={}&facets=[[\"categories:fabric\"],[\"project_type:mod\"]]",
        encoded_query
    );
    
    let resp = ureq::get(&url)
        .set("User-Agent", "antigravity/mc-server-gui (deepmind-agentic-coding)")
        .call()
        .map_err(|e| format!("Помилка HTTP-запиту: {}", e))?;
        
    let search_res: ModrinthSearchResponse = serde_json::from_reader(resp.into_reader())
        .map_err(|e| format!("Помилка парсингу JSON: {}", e))?;
        
    Ok(search_res.hits)
}

// Пошук збірок на Modrinth
pub fn search_modrinth_modpacks(query: &str) -> Result<Vec<ModrinthSearchHit>, String> {
    if query.trim().is_empty() {
        return Ok(Vec::new());
    }
    
    let encoded_query = urlencoding::encode(query);
    let url = format!(
        "https://api.modrinth.com/v2/search?query={}&facets=[[\"project_type:modpack\"]]",
        encoded_query
    );
    
    let resp = ureq::get(&url)
        .set("User-Agent", "antigravity/mc-server-gui (deepmind-agentic-coding)")
        .call()
        .map_err(|e| format!("Помилка HTTP-запиту: {}", e))?;
        
    let search_res: ModrinthSearchResponse = serde_json::from_reader(resp.into_reader())
        .map_err(|e| format!("Помилка парсингу JSON: {}", e))?;
        
    Ok(search_res.hits)
}

// Отримання сумісних версій з Modrinth
pub fn fetch_project_versions(project_id: &str, mc_version: &str, loader: &str) -> Result<Vec<ModrinthVersion>, String> {
    let url = format!("https://api.modrinth.com/v2/project/{}/version", project_id);
    let resp = ureq::get(&url)
        .set("User-Agent", "antigravity/mc-server-gui (deepmind-agentic-coding)")
        .call()
        .map_err(|e| format!("Помилка завантаження версій: {}", e))?;
        
    let versions: Vec<ModrinthVersion> = serde_json::from_reader(resp.into_reader())
        .map_err(|e| format!("Помилка парсингу JSON версій: {}", e))?;
        
    let filtered: Vec<ModrinthVersion> = versions
        .into_iter()
        .filter(|v| {
            let supports_mc = mc_version.is_empty() || v.game_versions.iter().any(|gv| gv == mc_version);
            let supports_loader = loader.is_empty() || v.loaders.iter().any(|l| l.eq_ignore_ascii_case(loader));
            supports_mc && supports_loader
        })
        .collect();
        
    Ok(filtered)
}

// CurseForge API Helpers
fn cf_endpoint(path: &str, api_key: &str) -> String {
    if api_key.trim().is_empty() {
        format!("https://api.curse.tools/v1{}", path)
    } else {
        format!("https://api.curseforge.com/v1{}", path)
    }
}

pub fn search_curseforge_projects(query: &str, loader: &str, api_key: &str) -> Result<Vec<CurseForgeMod>, String> {
    if query.trim().is_empty() {
        return Ok(Vec::new());
    }
    
    let loader_type_id = match loader {
        "forge" => 1,
        "neoforge" => 6,
        "fabric" => 4,
        _ => 0,
    };
    
    let encoded_query = urlencoding::encode(query);
    let mut path = format!("/mods/search?gameId=432&classId=6&searchFilter={}", encoded_query);
    if loader_type_id > 0 {
        path = format!("{}&modLoaderType={}", path, loader_type_id);
    }
    let url = cf_endpoint(&path, api_key);
    
    let mut req = ureq::get(&url)
        .set("User-Agent", "antigravity/mc-server-gui (deepmind-agentic-coding)");
        
    if !api_key.trim().is_empty() {
        req = req.set("x-api-key", api_key);
    }
    
    let resp = req.call().map_err(|e| format!("Помилка HTTP: {}", e))?;
    let res: CurseForgeSearchResponse = serde_json::from_reader(resp.into_reader())
        .map_err(|e| format!("Помилка парсингу JSON: {}", e))?;
        
    Ok(res.data)
}

pub fn fetch_curseforge_download_url(mod_id: u64, file_id: u64, api_key: &str) -> Result<String, String> {
    let url = cf_endpoint(&format!("/mods/{}/files/{}/download-url", mod_id, file_id), api_key);
    
    let mut req = ureq::get(&url)
        .set("User-Agent", "antigravity/mc-server-gui (deepmind-agentic-coding)");
        
    if !api_key.trim().is_empty() {
        req = req.set("x-api-key", api_key);
    }
    
    let resp = req.call().map_err(|e| format!("Помилка HTTP: {}", e))?;
    let res: CurseForgeDownloadUrlResponse = serde_json::from_reader(resp.into_reader())
        .map_err(|e| format!("Помилка парсингу JSON: {}", e))?;
        
    Ok(res.data)
}

pub fn fetch_curseforge_mod_details(mod_id: u64, api_key: &str) -> Result<CurseForgeMod, String> {
    let url = cf_endpoint(&format!("/mods/{}", mod_id), api_key);
    
    let mut req = ureq::get(&url)
        .set("User-Agent", "antigravity/mc-server-gui (deepmind-agentic-coding)");
        
    if !api_key.trim().is_empty() {
        req = req.set("x-api-key", api_key);
    }
    
    let resp = req.call().map_err(|e| format!("Помилка HTTP: {}", e))?;
    let res: CurseForgeModResponse = serde_json::from_reader(resp.into_reader())
        .map_err(|e| format!("Помилка парсингу JSON: {}", e))?;
        
    Ok(res.data)
}

// Завантаження файлу мода
pub fn download_mod_file(url: &str, server_path: &Path, filename: &str) -> Result<(), String> {
    let mods_dir = server_path.join("mods");
    if !mods_dir.exists() {
        fs::create_dir_all(&mods_dir).map_err(|e| format!("Не вдалося створити папку mods: {}", e))?;
    }
    
    let file_path = mods_dir.join(filename);
    
    let resp = ureq::get(url)
        .set("User-Agent", "antigravity/mc-server-gui (deepmind-agentic-coding)")
        .call()
        .map_err(|e| format!("Помилка завантаження файлу: {}", e))?;
        
    let mut reader = resp.into_reader();
    let mut file = fs::File::create(&file_path).map_err(|e| format!("Не вдалося створити файл: {}", e))?;
    std::io::copy(&mut reader, &mut file).map_err(|e| format!("Помилка збереження файлу: {}", e))?;
    
    Ok(())
}

// Завантаження чорного списку модів з файлу конфігурації
pub fn load_blacklist() -> Vec<String> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/zoozie".to_string());
    let path = std::path::PathBuf::from(home)
        .join(".config")
        .join("mc_server_manager")
        .join("blacklist.txt");

    if !path.exists() {
        let default_content = "\
# Чорний список модів для серверів (по одному слову на рядок).
# Будь-який мод, назва якого містить це слово (без врахування регістру), буде пропущено при встановленні збірки на сервер.

controlify
zoomify
e4mc
essential
playit
viafabric
viabackwards
viaversion
";
        let _ = fs::create_dir_all(path.parent().unwrap());
        let _ = fs::write(&path, default_content);
        return vec![
            "controlify".to_string(),
            "zoomify".to_string(),
            "e4mc".to_string(),
            "essential".to_string(),
            "playit".to_string(),
            "viafabric".to_string(),
            "viabackwards".to_string(),
            "viaversion".to_string(),
        ];
    }

    let mut list = Vec::new();
    if let Ok(content) = fs::read_to_string(&path) {
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            list.push(trimmed.to_lowercase());
        }
    }
    list
}

// Перевірка, чи мод є виключно клієнтським або входить до чорного списку
pub fn is_client_only_mod(jar_path: &Path) -> bool {
    let filename = jar_path.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
    
    // 1. Швидка фільтрація за назвою файлу для популярних клієнтських модів
    let client_only_substrings = [
        "sodium", "rubidium", "embeddium", "nvidium", "iris-", "oculus", "canvas-", "optifine", "optifabric",
        "dynamic-fps", "dynamicfps", "entityculling", "entity_texture_features",
        "animatica", "continuity", "cit-resewn", "citresewn", "zoomify", "distant-horizons", "distanthorizons",
        "presencefootsteps", "soundphysics", "sound-physics", "skinlayers", "skin-layers", "fabrishot",
        "bettergrassify", "better-grassify", "bettermounthud", "better-mount-hud", "custom-skin-loader",
        "customskinloader", "3dskinlayers", "main-menu", "borderless", "controlling-", "smoothboot",
        "smooth-boot", "fastquit", "isxander-main-menu", "credits", "modmenu", "mod-menu", "inventoryprofiles",
        "inventory-profiles", "itemphysic", "item-physic", "neat-", "appleskin", "apple-skin", "shulkerboxtooltip",
        "wthit", "had-enough-items", "jei-", "rei-", "emi-", "legendary-tooltips", "tooltips", "advancementinfo",
        "detail-armor-bar", "armorbar", "damage-tilt", "damagetilt", "sodiumextra", "sodium-extra",
        "reeses-sodium", "indium", "lambdabettergrass", "lambdynamicslights", "mousetweaks", "mouse-tweaks", "replaymod"
    ];
    
    for sub in &client_only_substrings {
        if filename.contains(sub) {
            return true;
        }
    }

    // 1.5. Чорний список (blacklist) з файлу конфігурації
    let blacklist = load_blacklist();
    let filename_lower = filename.to_lowercase();
    for b in &blacklist {
        let b_trimmed = b.trim();
        if !b_trimmed.is_empty() && !b_trimmed.starts_with('#') {
            if filename_lower.contains(&b_trimmed.to_lowercase()) {
                return true;
            }
        }
    }
    
    // 2. Глибокий аналіз вмісту JAR (читання fabric.mod.json та neoforge.mods.toml)
    if let Ok(file) = fs::File::open(jar_path) {
        if let Ok(mut archive) = zip::ZipArchive::new(file) {
            // Перевіряємо fabric.mod.json
            if let Ok(mut fabric_json) = archive.by_name("fabric.mod.json") {
                let mut content = String::new();
                if fabric_json.read_to_string(&mut content).is_ok() {
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                        if let Some(env) = val.get("environment") {
                            if env.as_str() == Some("client") {
                                return true;
                            }
                        }
                    }
                }
            }
            // Перевіряємо neoforge.mods.toml
            if let Ok(mut neoforge_toml) = archive.by_name("META-INF/neoforge.mods.toml") {
                let mut content = String::new();
                if neoforge_toml.read_to_string(&mut content).is_ok() {
                    if content.contains("side=\"CLIENT\"") || content.contains("side = \"CLIENT\"") {
                        return true;
                    }
                }
            }
        }
    }
    
    false
}

// Сканування та вилучення клієнтських модів із директорії mods сервера
pub fn purge_client_side_mods(server_path: &Path) -> Vec<String> {
    let mods_dir = server_path.join("mods");
    let mut removed = Vec::new();
    if !mods_dir.exists() {
        return removed;
    }
    
    if let Ok(entries) = fs::read_dir(&mods_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    if ext == "jar" && is_client_only_mod(&path) {
                        let filename = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                        let disabled_dir = mods_dir.join("client_mods_disabled");
                        let _ = fs::create_dir_all(&disabled_dir);
                        let dest = disabled_dir.join(&filename);
                        if fs::rename(&path, &dest).is_ok() || fs::remove_file(&path).is_ok() {
                            removed.push(filename);
                        }
                    }
                }
            }
        }
    }
    removed
}

// Вилучення іконки з JAR-файлу моду
pub fn extract_jar_icon(jar_path: &Path, cache_dir: &Path) -> Option<String> {
    let file = fs::File::open(jar_path).ok()?;
    let mut archive = zip::ZipArchive::new(file).ok()?;
    
    let mut found_name = None;
    for i in 0..archive.len() {
        if let Ok(file) = archive.by_index(i) {
            let name = file.name().to_lowercase();
            if name == "icon.png" || name == "logo.png" || name.ends_with("/icon.png") || name.ends_with("/logo.png") {
                found_name = Some(file.name().to_string());
                break;
            }
        }
    }
    
    if let Some(name) = found_name {
        if let Ok(mut file) = archive.by_name(&name) {
            let mut buf = Vec::new();
            if file.read_to_end(&mut buf).is_ok() {
                let filename = jar_path.file_name()?.to_string_lossy().to_string();
                let cache_filename = format!("{}.png", filename.strip_suffix(".jar").unwrap_or(&filename));
                let cache_file_path = cache_dir.join(cache_filename);
                
                if let Some(parent) = cache_file_path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                
                if fs::write(&cache_file_path, buf).is_ok() {
                    return Some(format!("file://{}", cache_file_path.to_string_lossy()));
                }
            }
        }
    }
    
    None
}

// Список встановлених модів
pub fn list_installed_mods(server_path: &Path) -> Vec<InstalledModInfo> {
    let metadata = load_installed_mods_metadata(server_path);
    let mods_dir = server_path.join("mods");
    let cache_dir = server_path.join(".cache").join("mod_icons");
    let mut installed = Vec::new();
    if mods_dir.exists() {
        if let Ok(entries) = fs::read_dir(mods_dir) {
            for entry in entries.flatten() {
                if let Some(ext) = entry.path().extension() {
                    if ext == "jar" {
                        let filename = entry.file_name().to_string_lossy().to_string();
                        let mut icon_url = metadata.iter()
                            .find(|m| m.filename == filename)
                            .and_then(|m| m.icon_url.clone());
                            
                        if icon_url.is_none() {
                            icon_url = extract_jar_icon(&entry.path(), &cache_dir);
                        }
                        
                        installed.push(InstalledModInfo { filename, icon_url });
                    }
                }
            }
        }
    }
    installed.sort_by(|a, b| a.filename.to_lowercase().cmp(&b.filename.to_lowercase()));
    installed
}

// Metadata saving and tracking for auto-updates
pub fn load_installed_mods_metadata(server_path: &Path) -> Vec<InstalledModMetadata> {
    let path = server_path.join("installed_mods_metadata.json");
    if path.exists() {
        if let Ok(content) = fs::read_to_string(path) {
            if let Ok(entries) = serde_json::from_str::<Vec<InstalledModMetadata>>(&content) {
                return entries;
            }
        }
    }
    Vec::new()
}

pub fn save_installed_mods_metadata(server_path: &Path, entries: &[InstalledModMetadata]) -> Result<(), std::io::Error> {
    let path = server_path.join("installed_mods_metadata.json");
    let content = serde_json::to_string_pretty(entries)?;
    fs::write(path, content)?;
    Ok(())
}

pub fn add_installed_mod_metadata(server_path: &Path, entry: InstalledModMetadata) {
    let mut entries = load_installed_mods_metadata(server_path);
    entries.retain(|e| e.filename != entry.filename);
    entries.push(entry);
    let _ = save_installed_mods_metadata(server_path, &entries);
}

pub fn remove_installed_mod_metadata(server_path: &Path, filename: &str) {
    let mut entries = load_installed_mods_metadata(server_path);
    entries.retain(|e| e.filename != filename);
    let _ = save_installed_mods_metadata(server_path, &entries);
}

// Видалення моду
pub fn delete_mod(server_path: &Path, filename: &str) -> std::io::Result<()> {
    let file_path = server_path.join("mods").join(filename);
    if file_path.exists() {
        fs::remove_file(file_path)?;
    }
    remove_installed_mod_metadata(server_path, filename);
    Ok(())
}

// Допоміжний метод для декодування URL імені файлу
fn url_decode(s: &str) -> String {
    let mut decoded = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '%' {
            let mut hex = String::new();
            if let Some(h1) = chars.next() { hex.push(h1); }
            if let Some(h2) = chars.next() { hex.push(h2); }
            if let Ok(b) = u8::from_str_radix(&hex, 16) {
                decoded.push(b as char);
            }
        } else {
            decoded.push(c);
        }
    }
    decoded
}

// Встановлення Modrinth Modpack (.mrpack) або CurseForge Modpack (.zip)
pub fn install_mrpack(
    mrpack_path: &Path,
    server_path: &Path,
    cf_api_key: &str,
    log_tx: &std::sync::mpsc::Sender<String>,
) -> Result<(), String> {
    let _ = log_tx.send(format!("[Modpack] Відкриття збірки: {:?}", mrpack_path));
    
    // 0. Резервне копіювання існуючої папки mods перед встановленням збірки
    let mods_dir = server_path.join("mods");
    if mods_dir.exists() {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let mods_old_dir = server_path.join(format!("mods_old_{}", timestamp));
        let _ = log_tx.send(format!("[Modpack] Знайдено існуючу папку mods. Перейменування на {:?} для запобігання конфліктам...", mods_old_dir));
        if let Err(e) = fs::rename(&mods_dir, &mods_old_dir) {
            let _ = log_tx.send(format!("[Modpack] [Попередження] Не вдалося перейменувати папку mods: {}", e));
        }
    }
    
    let zip_file = fs::File::open(mrpack_path).map_err(|e| format!("Не вдалося відкрити архів: {}", e))?;
    let mut archive = zip::ZipArchive::new(zip_file).map_err(|e| format!("Невірний zip-архів: {}", e))?;
    
    let is_curseforge = archive.by_name("manifest.json").is_ok();
    let is_modrinth = archive.by_name("modrinth.index.json").is_ok();
    
    let mut downloaded_count = 0;
    let mut overrides_dir = "overrides".to_string();
    
    if is_modrinth {
        let _ = log_tx.send("[Modpack] Виявлено формат Modrinth (.mrpack)".to_string());
        
        // 1. Парсимо modrinth.index.json
        let mut index_file = archive.by_name("modrinth.index.json")
            .map_err(|e| format!("Не знайдено modrinth.index.json в архіві: {}", e))?;
        let index: MrpackIndex = serde_json::from_reader(&mut index_file)
            .map_err(|e| format!("Помилка читання modrinth.index.json: {}", e))?;
        drop(index_file);
        
        let total_files = index.files.len();
        let _ = log_tx.send(format!("[Modpack] Знайдено {} файлів у списку залежностей.", total_files));
        
        // 2. Скачуємо залежності з пропусканням client-only
        for (idx, f) in index.files.iter().enumerate() {
            if let Some(ref env) = f.env {
                if env.server == "unsupported" {
                    let _ = log_tx.send(format!("[Modpack] [Пропуск] {} є виключно клієнтським модом.", f.path));
                    continue;
                }
            }
            
            let target_file_path = server_path.join(&f.path);
            if let Some(parent) = target_file_path.parent() {
                fs::create_dir_all(parent).map_err(|e| format!("Не вдалося створити директорію {:?}: {}", parent, e))?;
            }
            
            if f.downloads.is_empty() {
                let _ = log_tx.send(format!("[Modpack] [Попередження] Немає посилань для завантаження: {}", f.path));
                continue;
            }
            
            let url = &f.downloads[0];
            let _ = log_tx.send(format!("[Modpack] [{}/{}] Завантаження {}", idx + 1, total_files, f.path));
            
            let filename = target_file_path.file_name().unwrap().to_str().unwrap();
            if let Err(e) = download_mod_file(url, server_path, filename) {
                let _ = log_tx.send(format!("[Modpack] [Помилка] Не вдалося завантажити мод: {}", e));
                continue;
            }
            
            // Фільтрація клієнтських модів по самому JAR файлу
            if is_client_only_mod(&target_file_path) {
                let _ = log_tx.send(format!("[Modpack] [Пропуск] Видалено клієнтський мод після аналізу: {:?}", target_file_path.file_name().unwrap()));
                let _ = fs::remove_file(&target_file_path);
            } else {
                downloaded_count += 1;
            }
        }
    } else if is_curseforge {
        let _ = log_tx.send("[Modpack] Виявлено формат CurseForge (.zip)".to_string());
        
        // 1. Парсимо manifest.json
        let mut manifest_file = archive.by_name("manifest.json")
            .map_err(|e| format!("Не знайдено manifest.json в архіві: {}", e))?;
        let manifest: CurseForgeManifest = serde_json::from_reader(&mut manifest_file)
            .map_err(|e| format!("Помилка читання manifest.json: {}", e))?;
        drop(manifest_file);
        
        overrides_dir = manifest.overrides.clone();
        let total_files = manifest.files.len();
        let _ = log_tx.send(format!("[Modpack] Знайдено {} файлів у CurseForge списку.", total_files));
        
        // 2. Скачуємо залежності з CurseForge
        for (idx, f) in manifest.files.iter().enumerate() {
            let _ = log_tx.send(format!("[Modpack] [{}/{}] Запит URL для CurseForge FileID {}", idx + 1, total_files, f.file_id));
            
            let download_url = match fetch_curseforge_download_url(f.project_id, f.file_id, cf_api_key) {
                Ok(url) => url,
                Err(e) => {
                    let _ = log_tx.send(format!("[Modpack] [Помилка] Не вдалося отримати посилання для проекту {}: {}", f.project_id, e));
                    continue;
                }
            };
            
            let filename = download_url.split('/').last().unwrap_or("mod.jar").to_string();
            let decoded_filename = url_decode(&filename);
            
            let target_file_path = server_path.join("mods").join(&decoded_filename);
            if let Some(parent) = target_file_path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            
            let _ = log_tx.send(format!("[Modpack] Завантаження {}", decoded_filename));
            if let Err(e) = download_mod_file(&download_url, server_path, &decoded_filename) {
                let _ = log_tx.send(format!("[Modpack] [Помилка] Не вдалося завантажити {}: {}", decoded_filename, e));
                continue;
            }
            
            // Фільтрація клієнтських модів по самому JAR файлу
            if is_client_only_mod(&target_file_path) {
                let _ = log_tx.send(format!("[Modpack] [Пропуск] Видалено клієнтський мод після аналізу: {:?}", decoded_filename));
                let _ = fs::remove_file(&target_file_path);
            } else {
                downloaded_count += 1;
            }
        }
    } else {
        return Err("Архів не містить ні modrinth.index.json, ні manifest.json. Це не підтримувана збірка.".to_string());
    }
    
    // 3. Розпакування overrides та server-overrides
    let _ = log_tx.send("[Modpack] Застосування конфігурацій (overrides)...".to_string());
    
    // Для цього нам треба перевідкрити zip-архів
    let zip_file = fs::File::open(mrpack_path).map_err(|e| format!("Не вдалося відкрити архів: {}", e))?;
    let mut archive = zip::ZipArchive::new(zip_file).map_err(|e| format!("Невірний zip-архів: {}", e))?;
    
    let mut extracted_overrides = 0;
    let overrides_prefix = format!("{}/", overrides_dir);
    
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| format!("Помилка читання файлу в zip: {}", e))?;
        let name = file.name().to_string();
        
        let (is_override, rel_path) = if name.starts_with(&overrides_prefix) {
            (true, name.strip_prefix(&overrides_prefix).unwrap().to_string())
        } else if name.starts_with("overrides/") {
            (true, name.strip_prefix("overrides/").unwrap().to_string())
        } else if name.starts_with("server-overrides/") {
            (true, name.strip_prefix("server-overrides/").unwrap().to_string())
        } else {
            (false, String::new())
        };
        
        if is_override && !rel_path.is_empty() {
            let target_path = server_path.join(&rel_path);
            let is_jar_mod = rel_path.starts_with("mods/") && rel_path.ends_with(".jar");
            
            if file.is_dir() {
                fs::create_dir_all(&target_path)
                    .map_err(|e| format!("Не вдалося створити папку: {}", e))?;
            } else {
                if let Some(parent) = target_path.parent() {
                    fs::create_dir_all(parent)
                        .map_err(|e| format!("Не вдалося створити папку: {}", e))?;
                }
                
                let mut out_file = fs::File::create(&target_path)
                    .map_err(|e| format!("Не вдалося створити файл конфігурації: {}", e))?;
                std::io::copy(&mut file, &mut out_file)
                    .map_err(|e| format!("Помилка запису файлу конфігурації: {}", e))?;
                
                if is_jar_mod {
                    if is_client_only_mod(&target_path) {
                        let _ = log_tx.send(format!("[Modpack] [Пропуск] Видалено клієнтський мод з overrides: {:?}", target_path.file_name().unwrap()));
                        let _ = fs::remove_file(&target_path);
                    } else {
                        extracted_overrides += 1;
                    }
                } else {
                    extracted_overrides += 1;
                }
            }
        }
    }
    
    let _ = log_tx.send(format!(
        "[Modpack] Встановлення завершено успішно! Завантажено модів: {}, скопійовано конфігурацій: {}.",
        downloaded_count, extracted_overrides
    ));
    
    Ok(())
}

// Авто-оновлення модів
pub fn check_and_update_mods(
    server_path: &Path,
    mc_version: &str,
    loader: &str,
    cf_api_key: &str,
    log_tx: &std::sync::mpsc::Sender<String>,
) -> Result<(), String> {
    let _ = log_tx.send("[Updates] Перевірка оновлень для встановлених модів...".to_string());
    
    let entries = load_installed_mods_metadata(server_path);
    if entries.is_empty() {
        let _ = log_tx.send("[Updates] Немає відстежуваних модів для оновлення.".to_string());
        return Ok(());
    }
    
    let mut updated_entries = entries.clone();
    let mut updated_count = 0;
    
    for entry in &entries {
        let _ = log_tx.send(format!("[Updates] Перевірка {}...", entry.filename));
        if entry.source == "modrinth" {
            match fetch_project_versions(&entry.project_id, mc_version, loader) {
                Ok(versions) => {
                    if !versions.is_empty() {
                        let latest = &versions[0];
                        if latest.id != entry.version_id {
                            if !latest.files.is_empty() {
                                let file = latest.files.iter().find(|f| f.primary).unwrap_or(&latest.files[0]);
                                let _ = log_tx.send(format!("[Updates] Знайдено оновлення для Modrinth мода {}: {}", entry.project_id, file.filename));
                                
                                // Download new
                                if let Err(e) = download_mod_file(&file.url, server_path, &file.filename) {
                                    let _ = log_tx.send(format!("[Updates] Помилка завантаження {}: {}", file.filename, e));
                                    continue;
                                }
                                
                                // Delete old
                                let old_file_path = server_path.join("mods").join(&entry.filename);
                                if old_file_path.exists() && file.filename != entry.filename {
                                    let _ = fs::remove_file(old_file_path);
                                }
                                
                                // Update metadata entry
                                if let Some(idx) = updated_entries.iter().position(|e| e.filename == entry.filename) {
                                    updated_entries[idx] = InstalledModMetadata {
                                        filename: file.filename.clone(),
                                        source: "modrinth".to_string(),
                                        project_id: entry.project_id.clone(),
                                        version_id: latest.id.clone(),
                                        icon_url: entry.icon_url.clone(),
                                    };
                                }
                                
                                updated_count += 1;
                                let _ = log_tx.send(format!("[Updates] Мод {} успішно оновлено.", file.filename));
                            }
                        }
                    }
                }
                Err(e) => {
                    let _ = log_tx.send(format!("[Updates] Помилка перевірки оновлення для Modrinth {}: {}", entry.project_id, e));
                }
            }
        } else if entry.source == "curseforge" {
            let cf_loader_id = match loader {
                "forge" => Some(1),
                "neoforge" => Some(6),
                "fabric" => Some(4),
                _ => None,
            };
            if let Ok(mod_id) = entry.project_id.parse::<u64>() {
                match fetch_curseforge_mod_details(mod_id, cf_api_key) {
                    Ok(mod_details) => {
                        let mut compatible_files: Vec<CurseForgeFile> = mod_details.latest_files.into_iter()
                            .filter(|f| {
                                let supports_mc = f.game_versions.iter().any(|gv| gv == mc_version);
                                let supports_loader = cf_loader_id.is_none() || f.mod_loader_type == cf_loader_id;
                                supports_mc && supports_loader
                            })
                            .collect();
                            
                        compatible_files.sort_by(|a, b| b.id.cmp(&a.id));
                        
                        if !compatible_files.is_empty() {
                            let latest_file = &compatible_files[0];
                            let current_file_id = entry.version_id.parse::<u64>().unwrap_or(0);
                            
                            if latest_file.id > current_file_id {
                                let _ = log_tx.send(format!("[Updates] Знайдено оновлення для CurseForge мода {}: {}", mod_details.name, latest_file.file_name));
                                
                                let download_url = match latest_file.download_url {
                                    Some(ref url) => url.clone(),
                                    None => {
                                        match fetch_curseforge_download_url(mod_id, latest_file.id, cf_api_key) {
                                            Ok(url) => url,
                                            Err(e) => {
                                                let _ = log_tx.send(format!("[Updates] Не вдалося отримати URL для {}: {}", latest_file.file_name, e));
                                                continue;
                                            }
                                        }
                                    }
                                };
                                
                                // Download new
                                if let Err(e) = download_mod_file(&download_url, server_path, &latest_file.file_name) {
                                    let _ = log_tx.send(format!("[Updates] Помилка завантаження {}: {}", latest_file.file_name, e));
                                    continue;
                                }
                                
                                // Delete old
                                let old_file_path = server_path.join("mods").join(&entry.filename);
                                if old_file_path.exists() && latest_file.file_name != entry.filename {
                                    let _ = fs::remove_file(old_file_path);
                                }
                                
                                // Update metadata
                                if let Some(idx) = updated_entries.iter().position(|e| e.filename == entry.filename) {
                                    updated_entries[idx] = InstalledModMetadata {
                                        filename: latest_file.file_name.clone(),
                                        source: "curseforge".to_string(),
                                        project_id: entry.project_id.clone(),
                                        version_id: latest_file.id.to_string(),
                                        icon_url: entry.icon_url.clone(),
                                    };
                                }
                                
                                updated_count += 1;
                                let _ = log_tx.send(format!("[Updates] Мод {} успішно оновлено.", latest_file.file_name));
                            }
                        }
                    }
                    Err(e) => {
                        let _ = log_tx.send(format!("[Updates] Помилка перевірки оновлення для CurseForge {}: {}", entry.project_id, e));
                    }
                }
            }
        }
    }
    
    let _ = save_installed_mods_metadata(server_path, &updated_entries);
    let _ = log_tx.send(format!("[Updates] Перевірку завершено. Оновлено модів: {}.", updated_count));
    Ok(())
}

// Допоміжний модуль для простого кодування URL (щоб не тягнути додаткові залежності)
mod urlencoding {
    pub fn encode(data: &str) -> String {
        let mut escaped = String::new();
        for b in data.as_bytes() {
            match *b as char {
                'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '~' => escaped.push(*b as char),
                c => escaped.push_str(&format!("%{:02X}", c as u32)),
            }
        }
        escaped
    }
}

// Завантаження списку версій Minecraft з маніфесту Mojang
#[derive(Deserialize, Debug, Clone)]
pub struct MojangVersionEntry {
    pub id: String,
    #[serde(rename = "type")]
    pub version_type: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct MojangVersionManifest {
    pub versions: Vec<MojangVersionEntry>,
}

pub fn fetch_minecraft_versions() -> Result<(Vec<String>, Vec<String>), String> {
    let url = "https://launchermeta.mojang.com/mc/game/version_manifest_v2.json";
    let resp = ureq::get(url)
        .set("User-Agent", "antigravity/mc-server-gui (deepmind-agentic-coding)")
        .call()
        .map_err(|e| format!("Помилка завантаження версій: {}", e))?;
        
    let manifest: MojangVersionManifest = serde_json::from_reader(resp.into_reader())
        .map_err(|e| format!("Помилка парсингу маніфесту: {}", e))?;
        
    let mut releases = Vec::new();
    let mut snapshots = Vec::new();
    
    for v in manifest.versions {
        if v.version_type == "release" {
            releases.push(v.id);
        } else if v.version_type == "snapshot" {
            snapshots.push(v.id);
        }
    }
    
    Ok((releases, snapshots))
}

// Автоматичне визначення версії та ядра з локального файлу збірки (.mrpack чи CurseForge .zip)
pub fn extract_modpack_version_and_loader(pack_path: &Path) -> Result<(String, String), String> {
    let file = fs::File::open(pack_path).map_err(|e| format!("Не вдалося відкрити файл: {}", e))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("Невірний zip-архів: {}", e))?;
    
    if archive.by_name("modrinth.index.json").is_ok() {
        let mut index_file = archive.by_name("modrinth.index.json").unwrap();
        let val: serde_json::Value = serde_json::from_reader(&mut index_file)
            .map_err(|e| format!("Помилка читання modrinth.index.json: {}", e))?;
            
        let mc_version = val.get("dependencies")
            .and_then(|d| d.get("minecraft"))
            .and_then(|v| v.as_str())
            .unwrap_or("1.20.1")
            .to_string();
            
        let mut loader = "fabric".to_string();
        if let Some(deps) = val.get("dependencies") {
            if deps.get("forge").is_some() {
                loader = "forge".to_string();
            } else if deps.get("neoforge").is_some() {
                loader = "neoforge".to_string();
            }
        }
        return Ok((mc_version, loader));
    } else if archive.by_name("manifest.json").is_ok() {
        let mut manifest_file = archive.by_name("manifest.json").unwrap();
        let val: serde_json::Value = serde_json::from_reader(&mut manifest_file)
            .map_err(|e| format!("Помилка читання manifest.json: {}", e))?;
            
        let mc_version = val.get("minecraft")
            .and_then(|m| m.get("version"))
            .and_then(|v| v.as_str())
            .unwrap_or("1.20.1")
            .to_string();
            
        let mut loader = "fabric".to_string();
        if let Some(m) = val.get("minecraft") {
            if let Some(loaders) = m.get("modLoaders").and_then(|l| l.as_array()) {
                for l in loaders {
                    if let Some(id) = l.get("id").and_then(|id| id.as_str()) {
                        let id_lower = id.to_lowercase();
                        if id_lower.contains("neoforge") {
                            loader = "neoforge".to_string();
                            break;
                        } else if id_lower.contains("forge") {
                            loader = "forge".to_string();
                            break;
                        }
                    }
                }
            }
        }
        return Ok((mc_version, loader));
    }
    
    Err("Невідомий формат збірки (немає ні manifest.json, ні modrinth.index.json)".to_string())
}

// Завантаження файлу за вказаним шляхом
pub fn download_file(url: &str, dest_path: &Path) -> Result<(), String> {
    if let Some(parent) = dest_path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| format!("Не вдалося створити директорію: {}", e))?;
        }
    }
    
    let resp = ureq::get(url)
        .set("User-Agent", "antigravity/mc-server-gui (deepmind-agentic-coding)")
        .call()
        .map_err(|e| format!("Помилка HTTP: {}", e))?;
        
    let mut reader = resp.into_reader();
    let mut file = fs::File::create(dest_path).map_err(|e| format!("Не вдалося створити файл: {}", e))?;
    std::io::copy(&mut reader, &mut file).map_err(|e| format!("Помилка збереження файлу: {}", e))?;
    
    Ok(())
}

pub fn is_ely_by_skins_enabled(server_path: &Path) -> bool {
    let mods_dir = server_path.join("mods");
    if mods_dir.exists() {
        if let Ok(entries) = fs::read_dir(&mods_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.to_lowercase().contains("fabrictailor") && name.ends_with(".jar") {
                    return true;
                }
            }
        }
    }
    false
}

pub fn set_ely_by_skins_enabled(server_path: &Path, mc_version: &str, enable: bool) -> Result<(), String> {
    let mods_dir = server_path.join("mods");
    
    // 1. Спочатку перевіримо чи вже є встановлений FabricTailor
    let mut existing_file = None;
    if mods_dir.exists() {
        if let Ok(entries) = fs::read_dir(&mods_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.to_lowercase().contains("fabrictailor") && name.ends_with(".jar") {
                    existing_file = Some(entry.path());
                    break;
                }
            }
        }
    }
    
    if enable {
        if existing_file.is_none() {
            // Шукаємо сумісну версію на Modrinth
            let versions = match fetch_project_versions("fabrictailor", mc_version, "fabric") {
                Ok(v) => v,
                Err(_) => Vec::new(),
            };
            
            let version = if !versions.is_empty() {
                versions[0].clone()
            } else {
                // Якщо точного збігу для версії немає, спробуємо знайти будь-яку версію для fabric
                let url = "https://api.modrinth.com/v2/project/fabrictailor/version";
                let resp = ureq::get(url)
                    .set("User-Agent", "antigravity/mc-server-gui (deepmind-agentic-coding)")
                    .call()
                    .map_err(|e| format!("Не вдалося отримати версії FabricTailor: {}", e))?;
                let all_versions: Vec<ModrinthVersion> = serde_json::from_reader(resp.into_reader())
                    .map_err(|e| format!("Помилка парсингу версій FabricTailor: {}", e))?;
                
                let fabric_versions: Vec<ModrinthVersion> = all_versions
                    .into_iter()
                    .filter(|v| v.loaders.iter().any(|l| l.eq_ignore_ascii_case("fabric")))
                    .collect();
                
                if fabric_versions.is_empty() {
                    return Err("Не знайдено жодної версії FabricTailor для Fabric на Modrinth.".to_string());
                }
                fabric_versions[0].clone()
            };
            
            let primary_file = version.files.iter().find(|f| f.primary).unwrap_or(&version.files[0]);
            download_mod_file(&primary_file.url, server_path, &primary_file.filename)?;
        }
    } else {
        if let Some(path) = existing_file {
            fs::remove_file(path).map_err(|e| format!("Не вдалося видалити FabricTailor: {}", e))?;
        }
    }
    
    Ok(())
}
