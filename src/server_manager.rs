use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerStatus {
    Offline,
    Starting,
    Online,
}

#[allow(dead_code)]
pub struct ServerInstance {
    pub name: String,
    pub path: PathBuf,
    pub max_ram: u32,
    pub version: String,
    pub is_fabric: bool,
    pub loader_type: String,
    pub status: ServerStatus,
    pub log_buffer: String,
    pub child_process: Option<Child>,
    pub logs_sender: Sender<String>,
    pub logs_receiver: Receiver<String>,
    pub last_poll: Instant,
}

impl ServerInstance {
    pub fn new(name: String, path: PathBuf, max_ram: u32, version: String, loader_type: String) -> Self {
        let is_fabric = loader_type == "fabric";
        let (logs_sender, logs_receiver) = channel();
        Self {
            name,
            path,
            max_ram,
            version,
            is_fabric,
            loader_type,
            status: ServerStatus::Offline,
            log_buffer: String::new(),
            child_process: None,
            logs_sender,
            logs_receiver,
            last_poll: Instant::now() - Duration::from_secs(10),
        }
    }

    pub fn append_log(&mut self, text: String) {
        self.log_buffer.push_str(&text);
        self.log_buffer.push('\n');
        let lines: Vec<&str> = self.log_buffer.lines().collect();
        if lines.len() > 3000 {
            self.log_buffer = lines[lines.len() - 2000..].join("\n") + "\n";
        }
    }


    pub fn start(&mut self, ctx: &eframe::egui::Context) {
        if self.status != ServerStatus::Offline {
            return;
        }

        self.append_log("[GUI] Початок запуску сервера...".to_string());
        self.status = ServerStatus::Starting;
        self.last_poll = Instant::now();

        // 1. (Ngrok launch bypassed, using Tailscale)
        // 2. Визначаємо яке ядро запускати
        let mut jar_file = "server.jar".to_string();
        let mut use_run_sh = false;
        let run_sh_path = self.path.join("run.sh");
        if run_sh_path.exists() {
            use_run_sh = true;
            
            // Налаштовуємо RAM в user_jvm_args.txt
            let jvm_args_path = self.path.join("user_jvm_args.txt");
            let xmx_val = format!("-Xmx{}G", self.max_ram);
            let xms_val = format!("-Xms{}G", self.max_ram);
            let mut lines = Vec::new();
            if jvm_args_path.exists() {
                if let Ok(content) = fs::read_to_string(&jvm_args_path) {
                    for line in content.lines() {
                        let l = line.trim();
                        if !l.starts_with("-Xmx") && !l.starts_with("-Xms") {
                            lines.push(line.to_string());
                        }
                    }
                }
            }
            lines.push(xmx_val);
            lines.push(xms_val);
            let _ = fs::write(&jvm_args_path, lines.join("\n") + "\n");
        } else if self.path.join("fabric-server-launch.jar").exists() {
            jar_file = "fabric-server-launch.jar".to_string();
        } else if !self.path.join("server.jar").exists() {
            // Шукаємо будь-який інший jar
            if let Ok(entries) = fs::read_dir(&self.path) {
                for entry in entries.flatten() {
                    if let Some(ext) = entry.path().extension() {
                        if ext == "jar" && entry.file_name() != "fabric-installer.jar" {
                            jar_file = entry.file_name().to_string_lossy().to_string();
                            break;
                        }
                    }
                }
            }
        }

        self.append_log(format!("[GUI] Запуск сервера (use_run_sh: {})...", use_run_sh));

        let xmx = format!("-Xmx{}G", self.max_ram);
        let xms = format!("-Xms{}G", self.max_ram);
        
        let mut cmd = if use_run_sh {
            let mut c = Command::new("bash");
            c.arg("run.sh").arg("nogui");
            c
        } else {
            let mut c = Command::new("java");
            c.args([
                &xmx, &xms,
                "-XX:+UseG1GC", "-XX:+ParallelRefProcEnabled", "-XX:MaxGCPauseMillis=200",
                "-XX:+UnlockExperimentalVMOptions", "-XX:+AlwaysPreTouch",
                "-XX:+DisableExplicitGC", "-XX:+UseNUMA",
                "-jar", &jar_file, "nogui"
            ]);
            c
        };

        match cmd
            .current_dir(&self.path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(mut child) => {
                let stdout = child.stdout.take().unwrap();
                let stderr = child.stderr.take().unwrap();
                let tx_out = self.logs_sender.clone();
                let tx_err = self.logs_sender.clone();
                let ctx_clone = ctx.clone();

                // Потік читання stdout
                thread::spawn(move || {
                    let reader = BufReader::new(stdout);
                    for line in reader.lines() {
                        if let Ok(line_text) = line {
                            let _ = tx_out.send(line_text);
                            ctx_clone.request_repaint();
                        }
                    }
                });

                // Потік читання stderr
                let ctx_clone2 = ctx.clone();
                thread::spawn(move || {
                    let reader = BufReader::new(stderr);
                    for line in reader.lines() {
                        if let Ok(line_text) = line {
                            let _ = tx_err.send(line_text);
                            ctx_clone2.request_repaint();
                        }
                    }
                });

                self.child_process = Some(child);
                self.append_log("[GUI] Сервер запущено в фоні.".to_string());
            }
            Err(e) => {
                self.append_log(format!("[GUI] Помилка запуску Java: {}", e));
                self.status = ServerStatus::Offline;
            }
        }
    }

    pub fn get_ram_usage_mb(&self) -> Option<u64> {
        let child = self.child_process.as_ref()?;
        let pid = child.id();
        let path = format!("/proc/{}/status", pid);
        let content = fs::read_to_string(path).ok()?;
        for line in content.lines() {
            if line.starts_with("VmRSS:") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    if let Ok(kb) = parts[1].parse::<u64>() {
                        return Some(kb / 1024);
                    }
                }
            }
        }
        None
    }

    pub fn open_folder(&self) {
        let _ = Command::new("xdg-open")
            .arg(&self.path)
            .spawn();
    }

    pub fn reset_world_files(&self, seed: &str, hardcore: bool) -> Result<(), std::io::Error> {
        let level_name = self.read_property("level-name").unwrap_or_else(|| "world".to_string());
        let world_path = self.path.join(&level_name);
        if world_path.exists() {
            let _ = fs::remove_dir_all(&world_path);
        }
        let easyauth_path = self.path.join("EasyAuth");
        if easyauth_path.exists() {
            let _ = fs::remove_dir_all(&easyauth_path);
        }
        let _ = fs::remove_file(self.path.join("usercache.json"));
        let _ = fs::remove_file(self.path.join("whitelist.json"));
        let _ = fs::remove_file(self.path.join("ops.json"));

        self.write_property("level-seed", seed)?;
        if hardcore {
            self.write_property("hardcore", "true")?;
            self.write_property("difficulty", "hard")?;
        } else {
            self.write_property("hardcore", "false")?;
        }
        Ok(())
    }

    pub fn stop(&mut self) {
        if self.status == ServerStatus::Offline {
            return;
        }
        self.append_log("[GUI] Відправка команди stop на сервер...".to_string());
        if let Some(ref mut child) = self.child_process {
            if let Some(ref mut stdin) = child.stdin {
                let _ = writeln!(stdin, "stop");
                let _ = stdin.flush();
            }
        }
    }

    pub fn update_status(&mut self, ctx: &eframe::egui::Context) {
        let mut got_new_logs = false;
        while let Ok(line) = self.logs_receiver.try_recv() {
            self.append_log(line);
            got_new_logs = true;
        }
        if got_new_logs {
            ctx.request_repaint();
        }

        if let Some(ref mut child) = self.child_process {
            match child.try_wait() {
                Ok(Some(exit_status)) => {
                    self.append_log(format!("[GUI] Процес сервера завершився з кодом: {}", exit_status));
                    self.status = ServerStatus::Offline;
                    self.child_process = None;
                    ctx.request_repaint();
                }
                Ok(None) => {
                    // Сервер працює
                    if self.status == ServerStatus::Starting {
                        self.status = ServerStatus::Online;
                        ctx.request_repaint();
                    }
                }
                Err(e) => {
                    self.append_log(format!("[GUI] Помилка перевірки статусу: {}", e));
                }
            }
        }
    }

    // Робота з властивостями server.properties
    pub fn read_property(&self, key: &str) -> Option<String> {
        let path = self.path.join("server.properties");
        if !path.exists() {
            return None;
        }
        let content = fs::read_to_string(path).ok()?;
        for line in content.lines() {
            if line.starts_with(key) && line.contains('=') {
                if let Some(pos) = line.find('=') {
                    if line[..pos].trim() == key {
                        return Some(line[pos+1..].trim().to_string());
                    }
                }
            }
        }
        None
    }

    pub fn write_property(&self, key: &str, value: &str) -> Result<(), std::io::Error> {
        let path = self.path.join("server.properties");
        let content = if path.exists() {
            fs::read_to_string(&path)?
        } else {
            String::new()
        };

        let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
        let mut found = false;
        for line in &mut lines {
            if line.starts_with(key) && line.contains('=') {
                if let Some(pos) = line.find('=') {
                    if line[..pos].trim() == key {
                        *line = format!("{}={}", key, value);
                        found = true;
                        break;
                    }
                }
            }
        }
        if !found {
            lines.push(format!("{}={}", key, value));
        }
        fs::write(&path, lines.join("\n") + "\n")?;
        Ok(())
    }
}

pub fn get_forge_version(mc_version: &str) -> Result<String, String> {
    let url = "https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json";
    let resp = ureq::get(url)
        .call()
        .map_err(|e| format!("Помилка запиту Forge версій: {}", e))?;
    let body: serde_json::Value = serde_json::from_reader(resp.into_reader())
        .map_err(|e| format!("Помилка парсингу Forge версій: {}", e))?;
    
    let promos = body.get("promos")
        .ok_or_else(|| "Не знайдено секцію promos".to_string())?;
        
    let key_rec = format!("{}-recommended", mc_version);
    if let Some(v) = promos.get(&key_rec).and_then(|v| v.as_str()) {
        return Ok(v.to_string());
    }
    
    let key_lat = format!("{}-latest", mc_version);
    if let Some(v) = promos.get(&key_lat).and_then(|v| v.as_str()) {
        return Ok(v.to_string());
    }
    
    Err(format!("Не знайдено сумісної версії Forge для Minecraft {}", mc_version))
}

pub fn get_neoforge_version(mc_version: &str) -> Result<String, String> {
    let parts: Vec<&str> = mc_version.split('.').collect();
    if parts.len() < 2 || parts[0] != "1" {
        return Err(format!("Невідомий формат версії Minecraft: {}", mc_version));
    }
    let minor = parts[1];
    let patch = if parts.len() >= 3 { parts[2] } else { "0" };
    let prefix = format!("{}.{}.", minor, patch);

    let url = "https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml";
    let resp = ureq::get(url)
        .call()
        .map_err(|e| format!("Помилка запиту NeoForge версій: {}", e))?;
    let mut body = String::new();
    resp.into_reader().read_to_string(&mut body)
        .map_err(|e| format!("Помилка читання NeoForge версій: {}", e))?;
        
    let mut versions = Vec::new();
    for cap in body.split("<version>") {
        if let Some(end) = cap.find("</version>") {
            let v = cap[..end].trim();
            if v.starts_with(&prefix) && !v.contains("-beta") {
                versions.push(v.to_string());
            }
        }
    }
    
    if versions.is_empty() {
        return Err(format!("Не знайдено сумісної версії NeoForge для Minecraft {}", mc_version));
    }
    
    versions.sort_by(|a, b| {
        let parse_version = |s: &str| -> Vec<u32> {
            s.split('.')
                .map(|p| p.parse::<u32>().unwrap_or(0))
                .collect()
        };
        parse_version(a).cmp(&parse_version(b))
    });
    
    Ok(versions.last().unwrap().clone())
}

pub fn create_new_server(
    name: &str,
    target_path: &Path,
    version: &str,
    loader_type: &str,
    custom_jar_path: Option<&Path>,
    logs_tx: Sender<String>,
) -> Result<(), String> {
    let _ = logs_tx.send(format!("[GUI] Створення нового сервера '{}' ({}) у {:?}", name, loader_type, target_path));
    
    // 1. Створюємо папку
    fs::create_dir_all(target_path).map_err(|e| format!("Не вдалося створити папку: {}", e))?;
    
    // 2. Створюємо eula.txt
    fs::write(target_path.join("eula.txt"), "eula=true\n")
        .map_err(|e| format!("Не вдалося створити eula.txt: {}", e))?;
        
    // 3. Створюємо базовий server.properties
    let default_props = "\
difficulty=normal
gamemode=survival
hardcore=false
online-mode=false
level-name=world
server-port=25565
view-distance=16
white-list=true
enforce-whitelist=true
";
    if !target_path.join("server.properties").exists() {
        fs::write(target_path.join("server.properties"), default_props)
            .map_err(|e| format!("Не вдалося створити server.properties: {}", e))?;
    }

    if let Some(jar_path) = custom_jar_path {
        if !jar_path.exists() {
            return Err(format!("Вказаний .jar файл не існує: {:?}", jar_path));
        }
        
        let filename_lower = jar_path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase();
            
        if filename_lower.contains("installer") {
            let _ = logs_tx.send(format!("[GUI] Копіювання інсталятора {:?}...", jar_path));
            let installer_file = target_path.join("installer.jar");
            fs::copy(jar_path, &installer_file)
                .map_err(|e| format!("Не вдалося скопіювати .jar інсталятор: {}", e))?;

            let _ = logs_tx.send("[GUI] Запуск встановлення з локального інсталятора (це може зайняти деякий час)...".to_string());
            let status = Command::new("java")
                .args(["-jar", "installer.jar", "--installServer"])
                .current_dir(target_path)
                .status()
                .map_err(|e| format!("Не вдалося запустити інсталятор: {}", e))?;

            let _ = fs::remove_file(installer_file);

            if !status.success() {
                return Err("Інсталятор JAR завершився з помилкою".to_string());
            }
            let _ = logs_tx.send("[GUI] Сервер успішно встановлено з локального інсталятора!".to_string());
        } else {
            let target_jar_name = if filename_lower.contains("fabric") {
                "fabric-server-launch.jar"
            } else {
                "server.jar"
            };
            let dest_file = target_path.join(target_jar_name);
            let _ = logs_tx.send(format!("[GUI] Копіювання локального сервера у {:?}...", dest_file));
            fs::copy(jar_path, &dest_file)
                .map_err(|e| format!("Не вдалося скопіювати .jar файл: {}", e))?;
            let _ = logs_tx.send("[GUI] Локальний JAR файл успішно скопійовано!".to_string());
        }
    } else if loader_type == "fabric" {
        let _ = logs_tx.send("[GUI] Завантаження Fabric Installer...".to_string());
        let installer_url = "https://maven.fabricmc.net/net/fabricmc/fabric-installer/1.0.1/fabric-installer-1.0.1.jar";
        let resp = ureq::get(installer_url)
            .call()
            .map_err(|e| format!("Помилка завантаження Fabric Installer: {}", e))?;
            
        let mut data = Vec::new();
        resp.into_reader().read_to_end(&mut data)
            .map_err(|e| format!("Помилка читання Fabric Installer: {}", e))?;
            
        let installer_file = target_path.join("fabric-installer.jar");
        fs::write(&installer_file, data)
            .map_err(|e| format!("Помилка запису Fabric Installer: {}", e))?;
            
        let _ = logs_tx.send(format!("[GUI] Встановлення Fabric Server для версії {}...", version));
        
        let status = Command::new("java")
            .args([
                "-jar", "fabric-installer.jar",
                "server",
                "-mc", version,
                "-downloadMinecraft"
            ])
            .current_dir(target_path)
            .status()
            .map_err(|e| format!("Не вдалося запустити встановлювач Fabric: {}", e))?;
            
        let _ = fs::remove_file(installer_file);
        
        if !status.success() {
            return Err("Fabric Installer завершився з помилкою".to_string());
        }
        let _ = logs_tx.send("[GUI] Fabric Server успішно встановлено!".to_string());
    } else if loader_type == "forge" {
        let forge_ver = get_forge_version(version)?;
        let _ = logs_tx.send(format!("[GUI] Знайдено версію Forge: {}", forge_ver));
        let installer_url = format!(
            "https://maven.minecraftforge.net/net/minecraftforge/forge/{}-{}/forge-{}-{}-installer.jar",
            version, forge_ver, version, forge_ver
        );
        let _ = logs_tx.send(format!("[GUI] Завантаження Forge Installer з: {}", installer_url));
        
        let resp = ureq::get(&installer_url)
            .call()
            .map_err(|e| format!("Помилка завантаження Forge Installer: {}", e))?;
            
        let mut data = Vec::new();
        resp.into_reader().read_to_end(&mut data)
            .map_err(|e| format!("Помилка читання Forge Installer: {}", e))?;
            
        let installer_file = target_path.join("installer.jar");
        fs::write(&installer_file, data)
            .map_err(|e| format!("Помилка запису Forge Installer: {}", e))?;
            
        let _ = logs_tx.send("[GUI] Запуск встановлення Forge Server (це може зайняти деякий час)...".to_string());
        
        let status = Command::new("java")
            .args(["-jar", "installer.jar", "--installServer"])
            .current_dir(target_path)
            .status()
            .map_err(|e| format!("Не вдалося запустити Forge Installer: {}", e))?;
            
        let _ = fs::remove_file(installer_file);
        
        if !status.success() {
            return Err("Forge Installer завершився з помилкою".to_string());
        }
        let _ = logs_tx.send("[GUI] Forge Server успішно встановлено!".to_string());
    } else if loader_type == "neoforge" {
        let neo_ver = get_neoforge_version(version)?;
        let _ = logs_tx.send(format!("[GUI] Знайдено версію NeoForge: {}", neo_ver));
        let installer_url = format!(
            "https://maven.neoforged.net/releases/net/neoforged/neoforge/{}/neoforge-{}-installer.jar",
            neo_ver, neo_ver
        );
        let _ = logs_tx.send(format!("[GUI] Завантаження NeoForge Installer з: {}", installer_url));
        
        let resp = ureq::get(&installer_url)
            .call()
            .map_err(|e| format!("Помилка завантаження NeoForge Installer: {}", e))?;
            
        let mut data = Vec::new();
        resp.into_reader().read_to_end(&mut data)
            .map_err(|e| format!("Помилка читання NeoForge Installer: {}", e))?;
            
        let installer_file = target_path.join("installer.jar");
        fs::write(&installer_file, data)
            .map_err(|e| format!("Помилка запису NeoForge Installer: {}", e))?;
            
        let _ = logs_tx.send("[GUI] Запуск встановлення NeoForge Server (це може зайняти деякий час)...".to_string());
        
        let status = Command::new("java")
            .args(["-jar", "installer.jar", "--installServer"])
            .current_dir(target_path)
            .status()
            .map_err(|e| format!("Не вдалося запустити NeoForge Installer: {}", e))?;
            
        let _ = fs::remove_file(installer_file);
        
        if !status.success() {
            return Err("NeoForge Installer завершився з помилкою".to_string());
        }
        let _ = logs_tx.send("[GUI] NeoForge Server успішно встановлено!".to_string());
    } else {
        let _ = logs_tx.send(format!("[GUI] Завантаження Vanilla server.jar для версії {}...", version));
        
        let manifest_url = "https://launchermeta.mojang.com/mc/game/version_manifest_v2.json";
        let resp = ureq::get(manifest_url)
            .call()
            .map_err(|e| format!("Помилка завантаження маніфесту версій: {}", e))?;
            
        let manifest: serde_json::Value = serde_json::from_reader(resp.into_reader())
            .map_err(|e| format!("Помилка парсингу маніфесту: {}", e))?;
            
        let versions = manifest.get("versions").and_then(|v| v.as_array())
            .ok_or_else(|| "Невірний формат маніфесту".to_string())?;
            
        let version_entry = versions.iter()
            .find(|v| v.get("id").and_then(|id| id.as_str()) == Some(version))
            .ok_or_else(|| format!("Версію {} не знайдено в маніфесті Mojang", version))?;
            
        let version_json_url = version_entry.get("url").and_then(|u| u.as_str())
            .ok_or_else(|| "Не знайдено URL версії".to_string())?;
            
        let resp_meta = ureq::get(version_json_url)
            .call()
            .map_err(|e| format!("Помилка завантаження метаданих версії: {}", e))?;
            
        let meta: serde_json::Value = serde_json::from_reader(resp_meta.into_reader())
            .map_err(|e| format!("Помилка парсингу метаданих версії: {}", e))?;
            
        let server_download_url = meta.get("downloads")
            .and_then(|d| d.get("server"))
            .and_then(|s| s.get("url"))
            .and_then(|u| u.as_str())
            .ok_or_else(|| "Не знайдено URL завантаження server.jar".to_string())?;
            
        let resp_jar = ureq::get(server_download_url)
            .call()
            .map_err(|e| format!("Помилка завантаження server.jar: {}", e))?;
            
        let mut data = Vec::new();
        resp_jar.into_reader().read_to_end(&mut data)
            .map_err(|e| format!("Помилка читання server.jar: {}", e))?;
            
        fs::write(target_path.join("server.jar"), data)
            .map_err(|e| format!("Помилка запису server.jar: {}", e))?;
            
        let _ = logs_tx.send("[GUI] Vanilla server.jar успішно завантажено!".to_string());
    }

    Ok(())
}

fn walk_dir_recursive(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut result = Vec::new();
    let mut dirs_to_visit = vec![dir.to_path_buf()];
    while let Some(current_dir) = dirs_to_visit.pop() {
        if let Ok(entries) = fs::read_dir(current_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name == "backups" || name.starts_with("temp_") || name.contains("backup") {
                        continue;
                    }
                }
                result.push(path.clone());
                if path.is_dir() {
                    dirs_to_visit.push(path);
                }
            }
        }
    }
    Ok(result)
}

pub fn zip_dir(
    src_dir: &Path,
    dst_file: &Path,
    progress: &std::sync::atomic::AtomicU32,
    progress_start: u32,
    progress_end: u32,
    logs_tx: &Sender<String>,
) -> Result<(), String> {
    let file = fs::File::create(dst_file).map_err(|e| format!("Не вдалося створити файл бекапу: {}", e))?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::FileOptions::<()>::default()
        .compression_method(zip::CompressionMethod::Stored);

    let walkdir = walk_dir_recursive(src_dir)?;
    let total_entries = walkdir.len();
    if total_entries == 0 {
        progress.store(progress_end, std::sync::atomic::Ordering::Relaxed);
        return Ok(());
    }

    let range = progress_end.saturating_sub(progress_start);

    for (index, path) in walkdir.iter().enumerate() {
        let name = path.strip_prefix(src_dir)
            .map_err(|e| format!("Помилка шляху: {}", e))?;
        let name_str = name.to_string_lossy().replace('\\', "/");
        if path.is_file() {
            match fs::File::open(&path) {
                Ok(mut f) => {
                    if let Err(e) = zip.start_file(&*name_str, options) {
                        let _ = logs_tx.send(format!("[Backup] [Попередження] Не вдалося почати файл {}: {}", name_str, e));
                        continue;
                    }
                    let mut buffer = Vec::new();
                    if let Err(e) = f.read_to_end(&mut buffer) {
                        let _ = logs_tx.send(format!("[Backup] [Попередження] Не вдалося прочитати файл {}: {}", name_str, e));
                        continue;
                    }
                    if let Err(e) = zip.write_all(&buffer) {
                        let _ = logs_tx.send(format!("[Backup] [Попередження] Не вдалося записати вміст {}: {}", name_str, e));
                        continue;
                    }
                }
                Err(e) => {
                    let _ = logs_tx.send(format!("[Backup] [Попередження] Пропущено файл (не вдалося відкрити) {}: {}", name_str, e));
                }
            }
        } else if !name_str.is_empty() {
            if let Err(e) = zip.add_directory(&name_str, options) {
                let _ = logs_tx.send(format!("[Backup] [Попередження] Не вдалося додати папку {}: {}", name_str, e));
            }
        }

        let percentage = progress_start + (((index + 1) * range as usize) / total_entries) as u32;
        progress.store(percentage, std::sync::atomic::Ordering::Relaxed);
    }
    zip.finish().map_err(|e| format!("Не вдалося завершити zip-архів: {}", e))?;
    Ok(())
}

pub fn migrate_server(
    srv_name: &str,
    srv_path: &Path,
    srv_loader_type: &str,
    new_version: &str,
    modpack_path: Option<&Path>,
    cf_api_key: &str,
    logs_tx: Sender<String>,
    progress: std::sync::Arc<std::sync::atomic::AtomicU32>,
) -> Result<(), String> {
    let _ = logs_tx.send(format!("[Migration] Початок міграції сервера з версії на {}...", new_version));
    progress.store(0, std::sync::atomic::Ordering::Relaxed);

    // 1. Створення бекапу
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/zoozienix".to_string());
    let default_parent = std::path::PathBuf::from(home).join("Documents").join("minecraft_servers");
    let parent = srv_path.parent().unwrap_or(&default_parent);
    let backups_dir = parent.join("backups");
    if !backups_dir.exists() {
        fs::create_dir_all(&backups_dir).map_err(|e| format!("Не вдалося створити папку бекапів: {}", e))?;
    }

    let timestamp = Command::new("date")
        .arg("+%Y-%m-%d_%H-%M-%S")
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "backup".to_string());
    
    let backup_file = backups_dir.join(format!("{}_backup_{}.zip", srv_name.replace(" ", "_"), timestamp));
    let _ = logs_tx.send(format!("[Migration] Створення резервної копії у {:?}...", backup_file));

    if let Err(e) = zip_dir(srv_path, &backup_file, &progress, 0, 70, &logs_tx) {
        return Err(format!("Помилка створення бекапу: {}. Міграцію скасовано.", e));
    }
    let _ = logs_tx.send("[Migration] Резервну копію успішно створено!".to_string());
    progress.store(70, std::sync::atomic::Ordering::Relaxed);

    // 2. Очищення старих бібліотек, ядра та модів
    let _ = logs_tx.send("[Migration] Очищення старих файлів версії...".to_string());
    
    let files_to_delete = ["server.jar", "fabric-server-launch.jar", "installer.jar", "fabric-installer.jar"];
    for f in &files_to_delete {
        let p = srv_path.join(f);
        if p.exists() {
            let _ = fs::remove_file(p);
        }
    }

    let dirs_to_delete = ["libraries"];
    for d in &dirs_to_delete {
        let p = srv_path.join(d);
        if p.exists() {
            let _ = fs::remove_dir_all(p);
        }
    }

    let mods_dir = srv_path.join("mods");
    if mods_dir.exists() {
        let mods_old_dir = srv_path.join(format!("mods_old_{}", timestamp));
        let _ = logs_tx.send(format!("[Migration] Перейменування папки mods на {:?}...", mods_old_dir));
        if let Err(e) = fs::rename(&mods_dir, &mods_old_dir) {
            let _ = logs_tx.send(format!("[Migration] [Попередження] Не вдалося перейменувати mods: {}", e));
        }
    }
    progress.store(75, std::sync::atomic::Ordering::Relaxed);

    // 3. Запуск встановлення нової версії
    let _ = logs_tx.send(format!("[Migration] Встановлення нової версії {} ({})", new_version, srv_loader_type));
    create_new_server(srv_name, srv_path, new_version, srv_loader_type, None, logs_tx.clone())?;
    progress.store(85, std::sync::atomic::Ordering::Relaxed);

    // 4. Встановлення збірки (якщо вказано)
    if let Some(mp_path) = modpack_path {
        let _ = logs_tx.send(format!("[Migration] Встановлення збірки з {:?}", mp_path));
        crate::mod_manager::install_mrpack(mp_path, srv_path, cf_api_key, &logs_tx)?;
    }

    progress.store(100, std::sync::atomic::Ordering::Relaxed);
    let _ = logs_tx.send(format!("[Migration] Міграція на версію {} успішно завершена!", new_version));
    Ok(())
}
