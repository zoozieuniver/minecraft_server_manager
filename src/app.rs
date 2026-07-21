use std::io::Write;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use eframe::egui;

use crate::config::{AppConfig, ServerConfig};
use crate::server_manager::{self, ServerInstance, ServerStatus};
use crate::whitelist;
use crate::mod_manager;

struct CreatingServerState {
    name: String,
    path: String,
    version: String,
    loader_type: String,
    modpack_path: String,
    custom_jar_path: String,
    modpack_online_url: Option<String>,
    modpack_online_title: Option<String>,
    search_query: String,
    search_results: Vec<mod_manager::ModrinthSearchHit>,
    search_loading: bool,
    search_error: Option<String>,
    search_receiver: Option<Receiver<Result<Vec<mod_manager::ModrinthSearchHit>, String>>>,
    version_loading: bool,
    version_receiver: Option<Receiver<Result<(String, String, String), String>>>,
    log_buffer: String,
    logs_sender: Sender<String>,
    logs_receiver: Receiver<String>,
    rx_finish: Receiver<Result<ServerConfig, String>>,
    tx_finish: Sender<Result<ServerConfig, String>>,
    error_msg: Option<String>,
    is_working: bool,
}

pub struct MinecraftManagerApp {
    config: AppConfig,
    servers: Vec<ServerInstance>,
    active_tab: usize, // 0: Керування, 1: Whitelist, 2: Консоль, 3: Моди
    creating_state: Option<CreatingServerState>,
    
    // Поля введення для Whitelist
    whitelist_input: String,
    whitelist_cache: Vec<whitelist::WhitelistEntry>,
    whitelist_loaded_path: Option<PathBuf>,
    
    // Введення команд в консоль
    console_input: String,

    // Поля для видалення сервера
    delete_confirm: Option<usize>,
    delete_physically: bool,

    // Поля для скидання світу
    reset_seed: String,
    reset_hardcore: bool,
    reset_success_msg: Option<String>,
    reset_error_msg: Option<String>,

    // Поля для керування модами
    installed_mods: Vec<mod_manager::InstalledModInfo>,
    installed_mods_loaded_path: Option<PathBuf>,
    mod_search_query: String,
    mod_search_results: Vec<mod_manager::ModrinthSearchHit>,
    mod_search_loading: bool,
    mod_search_error: Option<String>,
    modpack_path_input: String,
    modpack_is_installing: bool,
    modpack_log_buffer: String,
    modpack_logs_receiver: Option<Receiver<String>>,
    modpack_logs_sender: Sender<String>,
    modpack_finish_receiver: Option<Receiver<Result<(), String>>>,
    
    // Канали для асинхронних запитів Modrinth
    search_receiver: Option<Receiver<Result<Vec<mod_manager::ModrinthSearchHit>, String>>>,
    download_receiver: Option<Receiver<Result<(), String>>>,
    download_status: Option<String>,

    // CurseForge search states
    cf_search_query: String,
    cf_search_results: Vec<mod_manager::CurseForgeMod>,
    cf_search_loading: bool,
    cf_search_error: Option<String>,
    cf_search_receiver: Option<Receiver<Result<Vec<mod_manager::CurseForgeMod>, String>>>,

    // Modpack online search states
    pack_search_query: String,
    pack_search_results: Vec<mod_manager::ModrinthSearchHit>,
    pack_search_loading: bool,
    pack_search_error: Option<String>,
    pack_search_receiver: Option<Receiver<Result<Vec<mod_manager::ModrinthSearchHit>, String>>>,
    pack_download_receiver: Option<Receiver<Result<(), String>>>,
    pack_download_status: Option<String>,

    // Auto-update states
    auto_update_is_running: bool,
    auto_update_log_buffer: String,
    auto_update_logs_receiver: Option<Receiver<String>>,
    auto_update_logs_sender: Sender<String>,
    auto_update_finish_receiver: Option<Receiver<Result<(), String>>>,
    
    // Активний підтаб у вкладці модів (0: Встановлені, 1: Modrinth, 2: CurseForge, 3: Збірки)
    active_mods_tab: usize,

    // Поля Tailscale
    tailscale_status: Option<crate::tailscale::TailscaleStatus>,
    tailscale_error: Option<String>,
    tailscale_loading: bool,
    tailscale_receiver: Option<std::sync::mpsc::Receiver<Result<crate::tailscale::TailscaleStatus, String>>>,
    last_tailscale_check: std::time::Instant,

    // Поля для міграції та бекапів
    migration_version_input: String,
    migration_is_running: bool,
    migration_log_buffer: String,
    migration_logs_receiver: Option<Receiver<String>>,
    migration_logs_sender: Sender<String>,
    migration_finish_receiver: Option<Receiver<Result<(String, String), String>>>,
    migration_progress: std::sync::Arc<std::sync::atomic::AtomicU32>,
    migration_modpack_path: String,
    migration_display_version: String,
    migration_use_modpack: bool,
    migration_include_snapshots: bool,
    all_releases: Vec<String>,
    all_snapshots: Vec<String>,
    versions_fetched: bool,
    version_fetch_receiver: Option<Receiver<Result<(Vec<String>, Vec<String>), String>>>,
}

impl MinecraftManagerApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Збільшуємо розмір шрифтів на 25% для кращої видимості
        let mut style = (*cc.egui_ctx.style()).clone();
        for text_style in style.text_styles.values_mut() {
            text_style.size *= 1.25;
        }
        cc.egui_ctx.set_style(style);

        let config = AppConfig::load();
        let mut servers = Vec::new();
        
        // Ініціалізуємо існуючі сервери з конфігурації
        for s in &config.servers {
            servers.push(ServerInstance::new(s.name.clone(), s.path.clone(), s.max_ram, s.version.clone(), s.loader_type.clone()));
        }

        let (modpack_logs_sender, rx_log) = channel();
        let (tx_update_log, rx_update_log) = channel();

        let (migration_logs_sender, rx_migration_log) = channel();

        // Запускаємо первинну перевірку Tailscale
        let (tx_ts, rx_ts) = channel();
        std::thread::spawn(move || {
            let res = crate::tailscale::query_tailscale_status();
            let _ = tx_ts.send(res);
        });

        // Запуск фонового потоку для завантаження версій Minecraft
        let (tx_versions, rx_versions) = channel();
        std::thread::spawn(move || {
            let res = mod_manager::fetch_minecraft_versions();
            let _ = tx_versions.send(res);
        });

        Self {
            config,
            servers,
            active_tab: 0,
            creating_state: None,
            whitelist_input: String::new(),
            whitelist_cache: Vec::new(),
            whitelist_loaded_path: None,
            console_input: String::new(),
            
            delete_confirm: None,
            delete_physically: false,
            
            reset_seed: "Сіб".to_string(),
            reset_hardcore: true,
            reset_success_msg: None,
            reset_error_msg: None,
            
            installed_mods: Vec::new(),
            installed_mods_loaded_path: None,
            mod_search_query: String::new(),
            mod_search_results: Vec::new(),
            mod_search_loading: false,
            mod_search_error: None,
            modpack_path_input: String::new(),
            modpack_is_installing: false,
            modpack_log_buffer: String::new(),
            modpack_logs_receiver: Some(rx_log),
            modpack_logs_sender,
            modpack_finish_receiver: None,
            
            search_receiver: None,
            download_receiver: None,
            download_status: None,

            cf_search_query: String::new(),
            cf_search_results: Vec::new(),
            cf_search_loading: false,
            cf_search_error: None,
            cf_search_receiver: None,

            pack_search_query: String::new(),
            pack_search_results: Vec::new(),
            pack_search_loading: false,
            pack_search_error: None,
            pack_search_receiver: None,
            pack_download_receiver: None,
            pack_download_status: None,

            auto_update_is_running: false,
            auto_update_log_buffer: String::new(),
            auto_update_logs_receiver: Some(rx_update_log),
            auto_update_logs_sender: tx_update_log,
            auto_update_finish_receiver: None,
            
            active_mods_tab: 0,

            tailscale_status: None,
            tailscale_error: None,
            tailscale_loading: true,
            tailscale_receiver: Some(rx_ts),
            last_tailscale_check: std::time::Instant::now(),

            migration_version_input: String::new(),
            migration_is_running: false,
            migration_log_buffer: String::new(),
            migration_logs_receiver: Some(rx_migration_log),
            migration_logs_sender: migration_logs_sender,
            migration_finish_receiver: None,
            migration_progress: std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
            migration_modpack_path: String::new(),
            migration_display_version: String::new(),
            migration_use_modpack: false,
            migration_include_snapshots: false,
            all_releases: Vec::new(),
            all_snapshots: Vec::new(),
            versions_fetched: false,
            version_fetch_receiver: Some(rx_versions),
        }
    }

    fn save_app_config(&self) {
        let _ = self.config.save();
    }

    fn get_active_server(&self) -> Option<&ServerInstance> {
        if self.config.selected_index < self.servers.len() {
            Some(&self.servers[self.config.selected_index])
        } else {
            None
        }
    }

    fn sync_whitelist_cache(&mut self, force: bool) {
        let path = if let Some(server) = self.get_active_server() {
            Some(server.path.clone())
        } else {
            None
        };
        
        if let Some(p) = path {
            if force || self.whitelist_loaded_path.as_ref() != Some(&p) {
                self.whitelist_cache = whitelist::load_whitelist(&p);
                self.whitelist_loaded_path = Some(p);
            }
        }
    }

    fn sync_installed_mods_cache(&mut self, force: bool) {
        let path = if let Some(server) = self.get_active_server() {
            Some(server.path.clone())
        } else {
            None
        };
        
        if let Some(p) = path {
            if force || self.installed_mods_loaded_path.as_ref() != Some(&p) {
                self.installed_mods = mod_manager::list_installed_mods(&p);
                self.installed_mods_loaded_path = Some(p);
            }
        }
    }
}

impl eframe::App for MinecraftManagerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 0. Опитування каналу завантаження версій Minecraft
        if let Some(ref rx) = self.version_fetch_receiver {
            if let Ok(res) = rx.try_recv() {
                match res {
                    Ok((releases, snapshots)) => {
                        self.all_releases = releases;
                        self.all_snapshots = snapshots;
                        self.versions_fetched = true;
                    }
                    Err(_) => {
                        // Fallback versions if offline
                        self.all_releases = vec![
                            "1.21.4".to_string(), "1.21.3".to_string(), "1.21.2".to_string(),
                            "1.21.1".to_string(), "1.21".to_string(),
                            "1.20.6".to_string(), "1.20.4".to_string(), "1.20.2".to_string(),
                            "1.20.1".to_string(), "1.20".to_string(),
                            "1.19.4".to_string(), "1.19.2".to_string(),
                            "1.18.2".to_string(), "1.17.1".to_string(), "1.16.5".to_string(),
                        ];
                        self.versions_fetched = true;
                    }
                }
                self.version_fetch_receiver = None;
            }
        }

        // Опитування логів міграції
        if let Some(ref rx) = self.migration_logs_receiver {
            while let Ok(log) = rx.try_recv() {
                self.migration_log_buffer.push_str(&log);
                self.migration_log_buffer.push('\n');
            }
        }

        // Опитування завершення міграції
        let mut migration_finished = false;
        let mut migration_result = None;
        if let Some(ref rx) = self.migration_finish_receiver {
            if let Ok(res) = rx.try_recv() {
                migration_finished = true;
                migration_result = Some(res);
            }
        }
        if migration_finished {
            self.migration_finish_receiver = None;
            self.migration_is_running = false;
            if let Some(res) = migration_result {
                match res {
                    Ok((new_version, new_loader)) => {
                        // Оновлюємо версію сервера в конфігу та в GUI
                        let selected_idx = self.config.selected_index;
                        if selected_idx < self.servers.len() {
                            self.servers[selected_idx].version = new_version.clone();
                            self.servers[selected_idx].loader_type = new_loader.clone();
                            self.config.servers[selected_idx].version = new_version;
                            self.config.servers[selected_idx].loader_type = new_loader;
                            let _ = self.config.save();
                        }
                        self.migration_log_buffer.push_str("\n[Migration] Міграцію успішно завершено!\n");
                    }
                    Err(e) => {
                        self.migration_log_buffer.push_str(&format!("\n[Migration] Помилка міграції: {}\n", e));
                    }
                }
            }
        }

        // Опитування каналу Tailscale
        if let Some(ref rx) = self.tailscale_receiver {
            if let Ok(res) = rx.try_recv() {
                self.tailscale_loading = false;
                match res {
                    Ok(status) => {
                        self.tailscale_status = Some(status);
                        self.tailscale_error = None;
                    }
                    Err(e) => {
                        self.tailscale_error = Some(e);
                        self.tailscale_status = None;
                    }
                }
                self.tailscale_receiver = None;
            }
        }

        // Періодичне опитування Tailscale (кожні 10 секунд)
        if self.last_tailscale_check.elapsed() > std::time::Duration::from_secs(10) && self.tailscale_receiver.is_none() {
            self.last_tailscale_check = std::time::Instant::now();
            self.tailscale_loading = true;
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let res = crate::tailscale::query_tailscale_status();
                let _ = tx.send(res);
            });
            self.tailscale_receiver = Some(rx);
        }

        // Оновлюємо статус процесів усіх серверів
        for server in &mut self.servers {
            server.update_status(ctx);
        }

        if let Some(ref mut state) = self.creating_state {
            if let Some(ref rx) = state.search_receiver {
                if let Ok(res) = rx.try_recv() {
                    state.search_loading = false;
                    match res {
                        Ok(hits) => {
                            state.search_results = hits;
                        }
                        Err(e) => {
                            state.search_error = Some(e);
                        }
                    }
                    state.search_receiver = None;
                }
            }

            if let Some(ref rx) = state.version_receiver {
                if let Ok(res) = rx.try_recv() {
                    state.version_loading = false;
                    match res {
                        Ok((mc_ver, loader, dl_url)) => {
                            state.version = mc_ver;
                            state.loader_type = if loader.eq_ignore_ascii_case("forge") {
                                "forge".to_string()
                            } else if loader.eq_ignore_ascii_case("neoforge") {
                                "neoforge".to_string()
                            } else if loader.eq_ignore_ascii_case("fabric") {
                                "fabric".to_string()
                            } else {
                                "vanilla".to_string()
                            };
                            state.modpack_online_url = Some(dl_url);
                        }
                        Err(e) => {
                            state.error_msg = Some(format!("Помилка аналізу збірки: {}", e));
                        }
                    }
                    state.version_receiver = None;
                }
            }
        }

        // Перевірка фонових пошуків Modrinth
        if let Some(ref rx) = self.search_receiver {
            if let Ok(res) = rx.try_recv() {
                self.mod_search_loading = false;
                match res {
                    Ok(hits) => {
                        self.mod_search_results = hits;
                    }
                    Err(e) => {
                        self.mod_search_error = Some(e);
                    }
                }
                self.search_receiver = None;
            }
        }

        // Перевірка фонових пошуків CurseForge
        if let Some(ref rx) = self.cf_search_receiver {
            if let Ok(res) = rx.try_recv() {
                self.cf_search_loading = false;
                match res {
                    Ok(mods) => {
                        self.cf_search_results = mods;
                    }
                    Err(e) => {
                        self.cf_search_error = Some(e);
                    }
                }
                self.cf_search_receiver = None;
            }
        }

        // Перевірка фонових пошуків збірок
        if let Some(ref rx) = self.pack_search_receiver {
            if let Ok(res) = rx.try_recv() {
                self.pack_search_loading = false;
                match res {
                    Ok(packs) => {
                        self.pack_search_results = packs;
                    }
                    Err(e) => {
                        self.pack_search_error = Some(e);
                    }
                }
                self.pack_search_receiver = None;
            }
        }

        // Перевірка фонових завантажень
        if let Some(ref rx) = self.download_receiver {
            if let Ok(res) = rx.try_recv() {
                match res {
                    Ok(_) => {
                        self.download_status = Some("Мод успішно встановлено!".to_string());
                        self.sync_installed_mods_cache(true);
                    }
                    Err(e) => {
                        self.download_status = Some(format!("Помилка встановлення: {}", e));
                    }
                }
                self.download_receiver = None;
            }
        }

        // Перевірка фонових завантажень збірок
        if let Some(ref rx) = self.pack_download_receiver {
            if let Ok(res) = rx.try_recv() {
                self.modpack_is_installing = false;
                match res {
                    Ok(_) => {
                        self.pack_download_status = Some("Збірку успішно встановлено!".to_string());
                        self.sync_installed_mods_cache(true);
                    }
                    Err(e) => {
                        self.pack_download_status = Some(format!("Помилка встановлення: {}", e));
                    }
                }
                self.pack_download_receiver = None;
            }
        }

        // Перевірка фонових логів встановлення збірок
        if let Some(ref rx) = self.modpack_logs_receiver {
            while let Ok(line) = rx.try_recv() {
                self.modpack_log_buffer.push_str(&line);
                self.modpack_log_buffer.push('\n');
            }
        }

        // Перевірка фінішу встановлення збірок
        if let Some(ref rx) = self.modpack_finish_receiver {
            if let Ok(res) = rx.try_recv() {
                self.modpack_is_installing = false;
                match res {
                    Ok(_) => {
                        self.modpack_log_buffer.push_str("[Modpack] Збірку успішно встановлено!\n");
                        self.sync_installed_mods_cache(true);
                    }
                    Err(e) => {
                        self.modpack_log_buffer.push_str(&format!("[Modpack] Помилка встановлення: {}\n", e));
                    }
                }
                self.modpack_finish_receiver = None;
            }
        }

        // Перевірка логів авто-оновлення
        if let Some(ref rx) = self.auto_update_logs_receiver {
            while let Ok(line) = rx.try_recv() {
                self.auto_update_log_buffer.push_str(&line);
                self.auto_update_log_buffer.push('\n');
            }
        }

        // Перевірка завершення авто-оновлення
        if let Some(ref rx) = self.auto_update_finish_receiver {
            if let Ok(res) = rx.try_recv() {
                self.auto_update_is_running = false;
                match res {
                    Ok(_) => {
                        self.auto_update_log_buffer.push_str("[Updates] Авто-оновлення успішно завершено!\n");
                        self.sync_installed_mods_cache(true);
                    }
                    Err(e) => {
                        self.auto_update_log_buffer.push_str(&format!("[Updates] Помилка авто-оновлення: {}\n", e));
                    }
                }
                self.auto_update_finish_receiver = None;
            }
        }

        // Перевіряємо закінчення створення нового сервера
        if let Some(ref mut state) = self.creating_state {
            // Зчитуємо логи встановлення
            while let Ok(line) = state.logs_receiver.try_recv() {
                state.log_buffer.push_str(&line);
                state.log_buffer.push('\n');
            }
            
            if let Ok(res) = state.rx_finish.try_recv() {
                state.is_working = false;
                match res {
                    Ok(new_server_config) => {
                        self.config.servers.push(new_server_config.clone());
                        self.servers.push(ServerInstance::new(
                            new_server_config.name.clone(),
                            new_server_config.path.clone(),
                            new_server_config.max_ram,
                            new_server_config.version.clone(),
                            new_server_config.loader_type.clone(),
                        ));
                        self.config.selected_index = self.servers.len() - 1;
                        self.save_app_config();
                        self.creating_state = None; // Закриваємо меню створення
                    }
                    Err(err) => {
                        state.error_msg = Some(err);
                    }
                }
            }
        }

        // Головний сайдбар (ліва панель)
        egui::SidePanel::left("left_panel").resizable(true).default_width(200.0).show(ctx, |ui| {
            ui.vertical(|ui| {
                ui.heading("📁 Мої Сервери");
                ui.separator();

                // Список серверів
                egui::ScrollArea::vertical().max_height(ui.available_height() - 50.0).show(ui, |ui| {
                    let mut to_remove = None;
                    for (i, s) in self.config.servers.iter().enumerate() {
                        let is_selected = self.config.selected_index == i && self.creating_state.is_none();
                        
                        // Кольорова цятка статусу
                        let status_color = if i < self.servers.len() {
                            match self.servers[i].status {
                                ServerStatus::Offline => egui::Color32::from_rgb(180, 50, 50),
                                ServerStatus::Starting => egui::Color32::from_rgb(180, 150, 50),
                                ServerStatus::Online => egui::Color32::from_rgb(50, 180, 50),
                            }
                        } else {
                            egui::Color32::GRAY
                        };

                        ui.horizontal(|ui| {
                            // Малюємо індикатор
                            let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                            ui.painter().circle_filled(rect.center(), 5.0, status_color);
                            
                            let button = ui.selectable_label(is_selected, &s.name);
                            if button.clicked() {
                                self.config.selected_index = i;
                                self.creating_state = None;
                                self.active_tab = 0;
                            }
                            
                            // Показуємо кнопку видалення, якщо сервер вимкнений
                            if i < self.servers.len() && self.servers[i].status == ServerStatus::Offline {
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.button("🗑").on_hover_text("Видалити сервер зі списку").clicked() {
                                        to_remove = Some(i);
                                    }
                                });
                            }
                        });
                    }

                    if let Some(idx) = to_remove {
                        self.delete_confirm = Some(idx);
                        self.delete_physically = false;
                    }
                });

                ui.separator();
                ui.add_space(5.0);

                // Кнопка створення нового сервера
                if ui.button("➕ Створити сервер").clicked() {
                    let (tx_log, rx_log) = channel();
                    let (tx_fin, rx_fin) = channel();
                    self.creating_state = Some(CreatingServerState {
                        name: String::new(),
                        path: {
                             let home = std::env::var("HOME").unwrap_or_else(|_| "/home/zoozienix".to_string());
                             format!("{}/Documents/minecraft_servers/new_server", home)
                         },
                        version: "26.2".to_string(),
                        loader_type: "fabric".to_string(),
                        modpack_path: String::new(),
                        custom_jar_path: String::new(),
                        modpack_online_url: None,
                        modpack_online_title: None,
                        search_query: String::new(),
                        search_results: Vec::new(),
                        search_loading: false,
                        search_error: None,
                        search_receiver: None,
                        version_loading: false,
                        version_receiver: None,
                        log_buffer: String::new(),
                        logs_sender: tx_log,
                        logs_receiver: rx_log,
                        rx_finish: rx_fin,
                        tx_finish: tx_fin,
                        error_msg: None,
                        is_working: false,
                    });
                }
            });
        });

        // Центральна панель (вміст обраного сервера або меню створення)
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(idx) = self.delete_confirm {
                ui.heading("⚠️ Підтвердження видалення сервера");
                ui.separator();
                ui.add_space(10.0);

                let server_name = self.config.servers[idx].name.clone();
                let server_path = self.config.servers[idx].path.clone();

                ui.label(format!("Ви дійсно бажаєте видалити сервер '{}'?", server_name));
                ui.label(format!("Директорія: {:?}", server_path));
                ui.add_space(10.0);

                ui.checkbox(&mut self.delete_physically, "🗑 Також фізично видалити папку з диска (безповоротно!)");
                ui.add_space(15.0);

                ui.horizontal(|ui| {
                    if ui.button("❌ Скасувати").clicked() {
                        self.delete_confirm = None;
                    }

                    if ui.button("🗑 Видалити").clicked() {
                        let path_to_delete = if self.delete_physically {
                            Some(server_path.clone())
                        } else {
                            None
                        };

                        // Видаляємо зі списку
                        self.config.servers.remove(idx);
                        if idx < self.servers.len() {
                            self.servers.remove(idx);
                        }
                        if self.config.selected_index >= self.config.servers.len() && !self.config.servers.is_empty() {
                            self.config.selected_index = self.config.servers.len() - 1;
                        }
                        self.save_app_config();

                        if let Some(p) = path_to_delete {
                            let _ = std::fs::remove_dir_all(p);
                        }

                        self.delete_confirm = None;
                    }
                });
                return;
            }

            if let Some(ref mut state) = self.creating_state {
                // МЕНЮ СТВОРЕННЯ СЕРВЕРА
                ui.heading("🆕 Створення нового сервера");
                ui.separator();
                ui.add_space(10.0);

                egui::Grid::new("create_grid")
                    .num_columns(2)
                    .spacing([10.0, 10.0])
                    .show(ui, |ui: &mut egui::Ui| {
                        ui.label("Назва сервера:");
                        ui.text_edit_singleline(&mut state.name);
                        ui.end_row();

                        ui.label("Папка (директорія):");
                        ui.horizontal(|ui| {
                            ui.text_edit_singleline(&mut state.path);
                            if ui.button("📁").on_hover_text("Вибрати папку через провідник").clicked() {
                                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                                    state.path = path.to_string_lossy().to_string();
                                }
                            }
                        });
                        ui.end_row();

                        ui.label("Версія Minecraft:");
                        ui.horizontal(|ui| {
                            let _r = ui.text_edit_singleline(&mut state.version);
                            if !state.version.is_empty() && !state.version.starts_with("1.") {
                                ui.colored_label(egui::Color32::from_rgb(220, 100, 100), "⚠ Невірний формат (має бути 1.x.x)")
                                    .on_hover_text("Версії Minecraft завжди починаються з '1.' (наприклад, '1.21.1').\nПри встановленні локальної збірки .mrpack виберіть її нижче, і версія/ядро заповняться автоматично.");
                            }
                            egui::ComboBox::from_id_source("mc_version_combo")
                                .selected_text("")
                                .show_ui(ui, |ui| {
                                    // Custom modpack versions
                                    for v in &["26.2", "26.1.2"] {
                                        ui.selectable_value(&mut state.version, v.to_string(), *v);
                                    }
                                    ui.separator();
                                    
                                    // Dynamic releases fetched online (or fallbacks)
                                    if self.versions_fetched {
                                        for v in self.all_releases.iter().take(20) {
                                            ui.selectable_value(&mut state.version, v.clone(), v);
                                        }
                                    } else {
                                        for v in &["1.22", "1.21.4", "1.21.3", "1.21.2", "1.21.1", "1.21", "1.20.6", "1.20.4", "1.20.1", "1.19.4", "1.18.2"] {
                                            ui.selectable_value(&mut state.version, v.to_string(), *v);
                                        }
                                    }
                                });
                        });
                        ui.end_row();

                        ui.label("Тип ядра:");
                        ui.horizontal(|ui: &mut egui::Ui| {
                            ui.radio_value(&mut state.loader_type, "fabric".to_string(), "Fabric");
                            ui.radio_value(&mut state.loader_type, "forge".to_string(), "Forge");
                            ui.radio_value(&mut state.loader_type, "neoforge".to_string(), "NeoForge");
                            ui.radio_value(&mut state.loader_type, "vanilla".to_string(), "Vanilla");
                        });
                        ui.end_row();

                        ui.label("Шлях до збірки .mrpack (локально):");
                        ui.horizontal(|ui| {
                            ui.text_edit_singleline(&mut state.modpack_path);
                            if ui.button("📁").on_hover_text("Вибрати файл .mrpack").clicked() {
                                if let Some(path) = rfd::FileDialog::new()
                                    .add_filter("Modpack", &["mrpack"])
                                    .pick_file() {
                                    state.modpack_path = path.to_string_lossy().to_string();
                                    if let Ok((ver, loader)) = mod_manager::extract_modpack_version_and_loader(&path) {
                                        state.version = ver;
                                        state.loader_type = loader;
                                    }
                                }
                            }
                        });
                        ui.end_row();

                        ui.label("Шлях до файлу сервера / інсталятора (.jar):");
                        ui.horizontal(|ui| {
                            ui.text_edit_singleline(&mut state.custom_jar_path);
                            if ui.button("📁").on_hover_text("Вибрати локальний .jar файл").clicked() {
                                if let Some(path) = rfd::FileDialog::new()
                                    .add_filter("JAR Executable", &["jar"])
                                    .pick_file() {
                                    state.custom_jar_path = path.to_string_lossy().to_string();
                                    
                                    // Auto-fill name and path if empty
                                    if state.name.trim().is_empty() {
                                        if let Some(stem) = path.file_stem() {
                                            state.name = stem.to_string_lossy().to_string();
                                            let home = std::env::var("HOME").unwrap_or_else(|_| "/home/zoozienix".to_string());
                                            state.path = format!("{}/Documents/minecraft_servers/{}", home, state.name);
                                        }
                                    }
                                    
                                    // Auto-detect loader type if installer jar
                                    let filename_lower = path.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
                                    if filename_lower.contains("neoforge") {
                                        state.loader_type = "neoforge".to_string();
                                    } else if filename_lower.contains("forge") {
                                        state.loader_type = "forge".to_string();
                                    } else if filename_lower.contains("fabric") {
                                        state.loader_type = "fabric".to_string();
                                    }
                                }
                            }
                        });
                        ui.end_row();
                    });

                ui.add_space(10.0);
                ui.separator();
                ui.heading("🔍 Знайти онлайн збірку на Modrinth");
                ui.horizontal(|ui| {
                    let text_edit = ui.text_edit_singleline(&mut state.search_query);
                    let mut search_clicked = false;
                    if text_edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        search_clicked = true;
                    }
                    if ui.button("🔍 Пошук").clicked() {
                        search_clicked = true;
                    }

                    if search_clicked && !state.search_query.trim().is_empty() {
                        state.search_loading = true;
                        state.search_error = None;
                        state.search_results.clear();
                        
                        let query = state.search_query.clone();
                        let (tx, rx) = channel();
                        state.search_receiver = Some(rx);
                        let ctx_clone = ctx.clone();
                        thread::spawn(move || {
                            let res = mod_manager::search_modrinth_modpacks(&query);
                            let _ = tx.send(res);
                            ctx_clone.request_repaint();
                        });
                    }
                });

                if state.search_loading {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Пошук модпаків...");
                    });
                }

                if let Some(ref err) = state.search_error {
                    ui.colored_label(egui::Color32::from_rgb(220, 50, 50), err);
                }

                if state.version_loading {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Аналіз збірки та автозаповнення параметрів...");
                    });
                }

                if let Some(ref title) = state.modpack_online_title {
                    ui.colored_label(egui::Color32::from_rgb(50, 180, 50), format!("✅ Вибрано збірку: {} (буде встановлена автоматично)", title));
                }

                if !state.search_results.is_empty() {
                    ui.add_space(5.0);
                    egui::ScrollArea::vertical().max_height(120.0).show(ui, |ui| {
                        for hit in &state.search_results {
                            ui.group(|ui| {
                                ui.horizontal(|ui| {
                                    if let Some(ref icon) = hit.icon_url {
                                        ui.add(egui::Image::new(icon).max_width(64.0).max_height(64.0));
                                    }
                                    ui.vertical(|ui| {
                                        ui.heading(&hit.title);
                                        ui.label(&hit.description);
                                        
                                        if ui.button("📋 Вибрати цю збірку").clicked() {
                                            state.version_loading = true;
                                            state.modpack_online_title = Some(hit.title.clone());
                                            
                                            let project_id = hit.project_id.clone();
                                            let (tx, rx) = channel();
                                            state.version_receiver = Some(rx);
                                            let ctx_clone = ctx.clone();
                                            thread::spawn(move || {
                                                let run = move || -> Result<(String, String, String), String> {
                                                    let versions = mod_manager::fetch_project_versions(&project_id, "", "")?;
                                                    if versions.is_empty() {
                                                        return Err("Не знайдено версій збірки".to_string());
                                                    }
                                                    let v = &versions[0];
                                                    if v.files.is_empty() {
                                                        return Err("Немає файлів у цій версії".to_string());
                                                    }
                                                    let mc_ver = v.game_versions.first().cloned().unwrap_or_else(|| "1.20.1".to_string());
                                                    let mut loader = "fabric".to_string();
                                                    for l in &v.loaders {
                                                        if l.eq_ignore_ascii_case("fabric") {
                                                            loader = "fabric".to_string();
                                                            break;
                                                        } else if l.eq_ignore_ascii_case("neoforge") {
                                                            loader = "neoforge".to_string();
                                                            break;
                                                        } else if l.eq_ignore_ascii_case("forge") {
                                                            loader = "forge".to_string();
                                                            break;
                                                        }
                                                    }
                                                    let url = v.files.first().map(|f| f.url.clone()).ok_or_else(|| "Немає URL файлу".to_string())?;
                                                    Ok((mc_ver, loader, url))
                                                };
                                                let _ = tx.send(run());
                                                ctx_clone.request_repaint();
                                            });
                                        }
                                    });
                                });
                            });
                        }
                    });
                }

                ui.add_space(15.0);

                let is_working = state.is_working;
                let mut create_clicked = false;
                let mut cancel_clicked = false;

                if is_working {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Будь ласка, зачекайте. Завантаження та встановлення сервера...");
                    });
                } else {
                    ui.horizontal(|ui| {
                        if ui.button("🚀 Створити сервер").clicked() {
                            create_clicked = true;
                        }

                        if ui.button("❌ Скасувати").clicked() {
                            cancel_clicked = true;
                        }
                    });
                }

                if cancel_clicked {
                    self.creating_state = None;
                    return;
                }

                if create_clicked {
                    if state.name.trim().is_empty() {
                        state.error_msg = Some("Введіть назву сервера!".to_string());
                    } else {
                        state.is_working = true;
                        state.error_msg = None;
                        state.log_buffer.clear();
                        
                        let name = state.name.clone();
                        let path = PathBuf::from(&state.path);
                        let version = state.version.clone();
                        let loader_type = state.loader_type.clone();
                        let modpack_path = state.modpack_path.clone();
                        let custom_jar_path = if state.custom_jar_path.trim().is_empty() {
                            None
                        } else {
                            Some(PathBuf::from(&state.custom_jar_path))
                        };
                        let modpack_online_url = state.modpack_online_url.clone();
                        let tx_log = state.logs_sender.clone();
                        let tx_fin = state.tx_finish.clone();
                        let cf_key = self.config.curseforge_key.clone();

                        thread::spawn(move || {
                            match server_manager::create_new_server(&name, &path, &version, &loader_type, custom_jar_path.as_deref(), tx_log.clone()) {
                                Ok(_) => {
                                    let mut final_modpack_path = None;
                                    if let Some(ref url) = modpack_online_url {
                                        let _ = tx_log.send("[GUI] Завантаження онлайн мод-збірки...".to_string());
                                        let temp_dest = path.join("temp_modpack.mrpack");
                                        match mod_manager::download_mod_file(url, &path, "temp_modpack.mrpack") {
                                            Ok(_) => {
                                                final_modpack_path = Some(temp_dest);
                                            }
                                            Err(e) => {
                                                let _ = tx_log.send(format!("[GUI ERROR] Не вдалося завантажити онлайн збірку: {}", e));
                                            }
                                        }
                                    } else if !modpack_path.trim().is_empty() {
                                        final_modpack_path = Some(PathBuf::from(&modpack_path));
                                    }

                                    if let Some(ref mpath) = final_modpack_path {
                                        let _ = tx_log.send("[GUI] Встановлення мод-збірки...".to_string());
                                        if let Err(e) = mod_manager::install_mrpack(mpath, &path, &cf_key, &tx_log) {
                                            let _ = tx_log.send(format!("[GUI ERROR] Помилка встановлення збірки: {}", e));
                                        }
                                        
                                        if modpack_online_url.is_some() {
                                            let _ = std::fs::remove_file(mpath);
                                        }
                                    }
                                    
                                    let is_fabric_bool = loader_type == "fabric";
                                    let _ = tx_fin.send(Ok(ServerConfig {
                                        name,
                                        path,
                                        max_ram: 8,
                                        version,
                                        is_fabric: is_fabric_bool,
                                        loader_type,
                                    }));
                                }
                                Err(err) => {
                                    let _ = tx_fin.send(Err(err));
                                }
                            }
                        });
                    }
                }

                if let Some(ref err) = state.error_msg {
                    ui.add_space(10.0);
                    ui.colored_label(egui::Color32::from_rgb(220, 50, 50), err);
                }

                ui.add_space(15.0);
                ui.label("Лог процесу встановлення:");
                
                let text_style = egui::TextStyle::Monospace;
                let row_height = ui.text_style_height(&text_style);
                egui::ScrollArea::vertical()
                    .max_height(200.0)
                    .stick_to_bottom(true)
                    .show_rows(ui, row_height, state.log_buffer.lines().count(), |ui, row_range| {
                        let lines: Vec<&str> = state.log_buffer.lines().collect();
                        let display_text = lines[row_range.start..row_range.end].join("\n");
                        ui.add(
                            egui::TextEdit::multiline(&mut display_text.clone())
                                .font(egui::FontId::monospace(10.0))
                                .desired_width(ui.available_width())
                                .desired_rows(10)
                        );
                    });

            } else if self.config.selected_index < self.servers.len() {
                let selected_idx = self.config.selected_index;
                
                // 1. Отримуємо копії даних для заголовка
                let (server_name, server_path) = {
                    let s = &self.servers[selected_idx];
                    (s.name.clone(), s.path.clone())
                };

                ui.horizontal(|ui| {
                    ui.heading(&server_name);
                    ui.label(format!("({:?})", server_path));
                });
                ui.separator();

                // 2. Таби (Вкладки)
                ui.horizontal(|ui: &mut egui::Ui| {
                    ui.selectable_value(&mut self.active_tab, 0, "⚙ Керування");
                    ui.selectable_value(&mut self.active_tab, 1, "👥 Whitelist");
                    ui.selectable_value(&mut self.active_tab, 2, "💻 Консоль");
                    ui.selectable_value(&mut self.active_tab, 3, "🔌 Моди");
                });
                ui.separator();

                match self.active_tab {
                    0 => {
                        // ВКЛАДКА КЕРУВАННЯ
                        ui.add_space(5.0);
                        
                        let status = self.servers[selected_idx].status;

                        ui.group(|ui| {
                            ui.horizontal(|ui| {
                                ui.label("Статус сервера:");
                                let (status_text, color) = match status {
                                    ServerStatus::Offline => ("ВИМКНЕНИЙ", egui::Color32::from_rgb(200, 50, 50)),
                                    ServerStatus::Starting => ("ЗАПУСКАЄТЬСЯ...", egui::Color32::from_rgb(200, 150, 50)),
                                    ServerStatus::Online => ("ПРАЦЮЄ", egui::Color32::from_rgb(50, 180, 50)),
                                };
                                ui.colored_label(color, status_text);
                            });

                            ui.add_space(5.0);

                            ui.horizontal(|ui| {
                                ui.label("Мережа Tailscale:");
                                if self.tailscale_loading {
                                    ui.spinner();
                                    ui.label("Отримання статусу...");
                                } else if let Some(ref err) = self.tailscale_error {
                                    ui.colored_label(egui::Color32::from_rgb(220, 50, 50), "Помилка");
                                    ui.label("❓").on_hover_text(err);
                                    if ui.button("⚡ Запустити").on_hover_text("Спробувати автоматично запустити службу tailscaled").clicked() {
                                        crate::tailscale::start_tailscale_daemon();
                                    }
                                } else if let Some(ref ts) = self.tailscale_status {
                                    let is_running = ts.backend_state == "Running";
                                    if is_running {
                                        ui.colored_label(egui::Color32::from_rgb(50, 180, 50), "ПРАЦЮЄ");
                                    } else {
                                        ui.colored_label(egui::Color32::from_rgb(200, 150, 50), &ts.backend_state);
                                    }
                                    if let Some(ref tailnet) = ts.current_tailnet {
                                        ui.label(format!("(Tailnet: {})", tailnet.name));
                                    }
                                }
                            });

                            if let Some(ref ts) = self.tailscale_status {
                                let mut ip_opt = None;
                                if let Some(ref ips) = ts.tailscale_ips {
                                    for ip in ips {
                                        if ip.contains('.') {
                                            ip_opt = Some(ip.clone());
                                            break;
                                        }
                                    }
                                }
                                if ip_opt.is_none() {
                                    if let Some(ref self_node) = ts.self_node {
                                        if let Some(ref ips) = self_node.tailscale_ips {
                                            for ip in ips {
                                                if ip.contains('.') {
                                                    ip_opt = Some(ip.clone());
                                                    break;
                                                }
                                            }
                                        }
                                    }
                                }

                                if let Some(ip) = ip_opt {
                                    ui.add_space(5.0);
                                    ui.horizontal(|ui| {
                                        ui.label("IP-адреса для друзів (Tailscale):");
                                        ui.text_edit_singleline(&mut ip.clone());
                                        if ui.button("📋 Скопіювати").clicked() {
                                            ui.output_mut(|o| o.copied_text = ip.clone());
                                        }
                                    });
                                }
                            }

                            // Трекер RAM
                            if status != ServerStatus::Offline {
                                ui.add_space(5.0);
                                if let Some(rss_mb) = self.servers[selected_idx].get_ram_usage_mb() {
                                    let max_gb = self.servers[selected_idx].max_ram as f32;
                                    let rss_gb = rss_mb as f32 / 1024.0;
                                    let fraction = (rss_gb / max_gb).clamp(0.0, 1.0);
                                    ui.horizontal(|ui| {
                                        ui.label("Використання ОЗП:");
                                        let progress_bar = egui::ProgressBar::new(fraction)
                                            .text(format!("{:.2} GB / {:.2} GB ({:.1}%)", rss_gb, max_gb, fraction * 100.0));
                                        ui.add(progress_bar);
                                    });
                                }
                            }
                        });

                        ui.add_space(10.0);

                        // Кнопки Старту / Стопу / Провідника
                        ui.horizontal(|ui| {
                            let is_offline = status == ServerStatus::Offline;
                            
                            if ui.add_enabled(is_offline, egui::Button::new("▶ Запустити").min_size(egui::vec2(100.0, 30.0))).clicked() {
                                self.servers[selected_idx].start(ctx);
                            }

                            if ui.add_enabled(!is_offline, egui::Button::new("🛑 Зупинити").min_size(egui::vec2(100.0, 30.0))).clicked() {
                                self.servers[selected_idx].stop();
                            }

                             if ui.add(egui::Button::new("📂 Відкрити папку").min_size(egui::vec2(120.0, 30.0))).clicked() {
                                self.servers[selected_idx].open_folder();
                            }
                        });

                        ui.add_space(10.0);

                        // Налаштування ОЗП
                        let is_offline = status == ServerStatus::Offline;
                        ui.group(|ui| {
                            ui.label("⚙ Налаштування ОЗП:");
                            ui.horizontal(|ui| {
                                ui.label("Максимум оперативної пам'яті:");
                                let mut ram = self.config.servers[selected_idx].max_ram;
                                if ui.add_enabled(is_offline, egui::Slider::new(&mut ram, 1..=32).suffix(" GB")).changed() {
                                    self.config.servers[selected_idx].max_ram = ram;
                                    self.servers[selected_idx].max_ram = ram;
                                    self.save_app_config();
                                }
                            });
                            if !is_offline {
                                ui.colored_label(egui::Color32::from_rgb(150, 150, 150), "Зміна пам'яті доступна тільки коли сервер вимкнений.");
                            }
                        });

                        ui.add_space(15.0);
                        ui.heading("Налаштування світу (server.properties)");
                        ui.separator();
                        ui.add_space(5.0);

                        if is_offline {
                            let srv = &self.servers[selected_idx];
                            let current_gamemode = srv.read_property("gamemode").unwrap_or_else(|| "survival".to_string());
                            let current_difficulty = srv.read_property("difficulty").unwrap_or_else(|| "normal".to_string());
                            let current_hardcore = srv.read_property("hardcore").unwrap_or_else(|| "false".to_string()) == "true";
                            let current_whitelist = srv.read_property("white-list").unwrap_or_else(|| "false".to_string()) == "true";
                            let current_view_distance_str = srv.read_property("view-distance").unwrap_or_else(|| "10".to_string());
                            let current_simulation_distance_str = srv.read_property("simulation-distance").unwrap_or_else(|| "10".to_string());
                            let current_view_distance = current_view_distance_str.trim().parse::<u32>().unwrap_or(10);
                            let current_simulation_distance = current_simulation_distance_str.trim().parse::<u32>().unwrap_or(10);

                            egui::Grid::new("settings_grid")
                                .num_columns(2)
                                .spacing([10.0, 10.0])
                                .show(ui, |ui: &mut egui::Ui| {
                                    ui.horizontal(|ui| {
                                        ui.label("Режим гри:");
                                        ui.label("❓").on_hover_text("Визначає початковий режим гри для нових гравців (survival - Виживання, creative - Творчий, adventure - Пригодницький, spectator - Спостерігач).");
                                    });
                                    ui.horizontal(|ui: &mut egui::Ui| {
                                        for mode in &["survival", "creative", "adventure", "spectator"] {
                                            if ui.selectable_label(&current_gamemode == mode, *mode).clicked() {
                                                let _ = srv.write_property("gamemode", mode);
                                            }
                                        }
                                    });
                                    ui.end_row();

                                    ui.horizontal(|ui| {
                                        ui.label("Складність:");
                                        ui.label("❓").on_hover_text("Регулює рівень шкоди від ворожих мобів та силу ефектів голоду. На 'peaceful' ворожі моби взагалі не спавняться.");
                                    });
                                    ui.horizontal(|ui: &mut egui::Ui| {
                                        for diff in &["peaceful", "easy", "normal", "hard"] {
                                            if ui.selectable_label(&current_difficulty == diff && !current_hardcore, *diff).clicked() {
                                                let _ = srv.write_property("difficulty", diff);
                                                let _ = srv.write_property("hardcore", "false");
                                            }
                                        }
                                        if ui.selectable_label(current_hardcore, "hardcore (Hard)").clicked() {
                                            let _ = srv.write_property("difficulty", "hard");
                                            let _ = srv.write_property("hardcore", "true");
                                        }
                                    });
                                    ui.end_row();

                                    ui.horizontal(|ui| {
                                        ui.label("Дальність рендеру (View Distance):");
                                        ui.label("❓").on_hover_text("Визначає кількість чанків навколо гравця, які завантажуються та відображаються клієнтом.");
                                    });
                                    ui.horizontal(|ui| {
                                        let mut val = current_view_distance;
                                        if ui.add(egui::Slider::new(&mut val, 2..=32).suffix(" чанків")).changed() {
                                            let _ = srv.write_property("view-distance", &val.to_string());
                                        }
                                    });
                                    ui.end_row();

                                    ui.horizontal(|ui| {
                                        ui.label("Дальність симуляції:");
                                        ui.label("❓").on_hover_text("Визначає дальність у чанках навколо гравця, в межах якої оновлюються моби та процеси.");
                                    });
                                    ui.horizontal(|ui| {
                                        let mut val = current_simulation_distance;
                                        if ui.add(egui::Slider::new(&mut val, 2..=32).suffix(" чанків")).changed() {
                                            let _ = srv.write_property("simulation-distance", &val.to_string());
                                        }
                                    });
                                    ui.end_row();

                                    ui.horizontal(|ui| {
                                        ui.label("Білий список (whitelist):");
                                        ui.label("❓").on_hover_text("Якщо увімкнено, до сервера зможуть підключитися лише ті гравці, які додані до whitelist.");
                                    });
                                    ui.horizontal(|ui| {
                                        let mut wl = current_whitelist;
                                        if ui.checkbox(&mut wl, "Увімкнути whitelist").changed() {
                                            let _ = srv.write_property("white-list", if wl { "true" } else { "false" });
                                        }
                                    });
                                    ui.end_row();

                                    ui.horizontal(|ui| {
                                        ui.label("Скіни Ely.by (FabricTailor):");
                                        ui.label("❓").on_hover_text("Автоматично завантажує та підключає мод FabricTailor для відображення скінів Ely.by та Mojang.");
                                    });
                                    ui.horizontal(|ui| {
                                        let is_skins_enabled = mod_manager::is_ely_by_skins_enabled(&srv.path);
                                        let mut skins = is_skins_enabled;
                                        if ui.checkbox(&mut skins, "Активувати скіни").changed() {
                                            let srv_path = srv.path.clone();
                                            let mc_ver = srv.version.clone();
                                            std::thread::spawn(move || {
                                                let _ = mod_manager::set_ely_by_skins_enabled(&srv_path, &mc_ver, skins);
                                            });
                                        }
                                    });
                                    ui.end_row();
                                });

                            // Секція Резервні копії та міграція версії
                            ui.add_space(15.0);
                            ui.heading("🚀 Резервні копії та міграція версії");
                            ui.separator();
                            ui.add_space(5.0);

                            ui.group(|ui| {
                                ui.label("Тут ви можете створити резервну копію (бекап) сервера або мігрувати на іншу версію.");

                                ui.horizontal(|ui| {
                                    ui.label("Поточна версія:");
                                    ui.strong(&self.servers[selected_idx].version);
                                    ui.label(format!("({})", self.servers[selected_idx].loader_type));
                                });

                                ui.add_space(5.0);

                                // Кнопка бекапу
                                if ui.add_enabled(!self.migration_is_running, egui::Button::new("📦 Створити повний бекап сервера")).clicked() {
                                    let srv_name = self.servers[selected_idx].name.clone();
                                    let srv_path = self.servers[selected_idx].path.clone();
                                    let logs_tx = self.migration_logs_sender.clone();
                                    let (finish_tx, finish_rx) = std::sync::mpsc::channel();
                                    let progress = self.migration_progress.clone();

                                    self.migration_is_running = true;
                                    self.migration_progress.store(0, std::sync::atomic::Ordering::Relaxed);
                                    self.migration_log_buffer = "[Backup] Початок створення резервної копії...\n".to_string();
                                    self.migration_finish_receiver = Some(finish_rx);
                                    let ctx_clone = ctx.clone();

                                    std::thread::spawn(move || {
                                         let home = std::env::var("HOME").unwrap_or_else(|_| "/home/zoozienix".to_string());
                                         let default_parent = std::path::PathBuf::from(home).join("Documents").join("minecraft_servers");
                                         let parent = srv_path.parent().unwrap_or(&default_parent);
                                        let backups_dir = parent.join("backups");
                                        if !backups_dir.exists() {
                                            let _ = std::fs::create_dir_all(&backups_dir);
                                        }

                                        let timestamp = std::process::Command::new("date")
                                            .arg("+%Y-%m-%d_%H-%M-%S")
                                            .output()
                                            .ok()
                                            .and_then(|out| String::from_utf8(out.stdout).ok())
                                            .map(|s| s.trim().to_string())
                                            .unwrap_or_else(|| "backup".to_string());

                                        let backup_file = backups_dir.join(format!("{}_manual_backup_{}.zip", srv_name.replace(" ", "_"), timestamp));
                                        let _ = logs_tx.send(format!("[Backup] Створення резервної копії у {:?}...", backup_file));

                                        match crate::server_manager::zip_dir(&srv_path, &backup_file, &progress, 0, 100, &logs_tx) {
                                            Ok(_) => {
                                                let _ = logs_tx.send("[Backup] Резервну копію успішно створено!".to_string());
                                                let _ = finish_tx.send(Ok((String::new(), String::new())));
                                            }
                                            Err(e) => {
                                                let _ = logs_tx.send(format!("[Backup] Помилка створення копії: {}", e));
                                                let _ = finish_tx.send(Err(e));
                                            }
                                        }
                                        ctx_clone.request_repaint();
                                    });
                                }

                                ui.add_space(10.0);
                                ui.separator();
                                ui.add_space(5.0);

                                ui.label(egui::RichText::new("Міграція версії сервера").strong());

                                // Перемикач типу міграції
                                ui.horizontal(|ui| {
                                    ui.selectable_value(&mut self.migration_use_modpack, false, "Чиста версія");
                                    ui.selectable_value(&mut self.migration_use_modpack, true, "Через файл збірки (.mrpack / .zip)");
                                });

                                ui.add_space(5.0);

                                if !self.migration_use_modpack {
                                    // Чиста міграція
                                    ui.horizontal(|ui| {
                                        ui.label("Цільова версія для міграції:");
                                        if !self.versions_fetched {
                                            ui.spinner();
                                            ui.label("Завантаження версій...");
                                        } else {
                                            let current_version = &self.servers[selected_idx].version;
                                            let mut versions_to_show = Vec::new();
                                            let src_list = if self.migration_include_snapshots {
                                                let mut all = self.all_releases.clone();
                                                all.extend(self.all_snapshots.clone());
                                                all
                                            } else {
                                                self.all_releases.clone()
                                            };

                                            for v in &src_list {
                                                if is_version_newer(current_version, v) {
                                                    versions_to_show.push(v.clone());
                                                }
                                            }

                                            if versions_to_show.is_empty() {
                                                ui.label(egui::RichText::new("Немає новіших версій для міграції").weak());
                                            } else {
                                                if self.migration_version_input.is_empty() || !versions_to_show.contains(&self.migration_version_input) {
                                                    self.migration_version_input = versions_to_show[0].clone();
                                                }
                                                egui::ComboBox::from_id_source("migration_version_select")
                                                    .selected_text(&self.migration_version_input)
                                                    .show_ui(ui, |ui| {
                                                        for v in &versions_to_show {
                                                            ui.selectable_value(&mut self.migration_version_input, v.clone(), v);
                                                        }
                                                    });
                                            }

                                            ui.checkbox(&mut self.migration_include_snapshots, "Включати снапшоти");
                                        }
                                    });
                                } else {
                                    // Міграція через збірку
                                    egui::Grid::new("migration_modpack_grid")
                                        .num_columns(2)
                                        .spacing([10.0, 10.0])
                                        .show(ui, |ui| {
                                            ui.label("Файл збірки:");
                                            ui.horizontal(|ui| {
                                                let text_edit = ui.text_edit_singleline(&mut self.migration_modpack_path);
                                                if text_edit.changed() && !self.migration_modpack_path.trim().is_empty() {
                                                    let mp_path = std::path::Path::new(self.migration_modpack_path.trim());
                                                    if mp_path.exists() {
                                                        if let Ok((ver, _loader)) = mod_manager::extract_modpack_version_and_loader(mp_path) {
                                                             self.migration_version_input = ver;
                                                             if self.migration_display_version.trim().is_empty() {
                                                                 if let Some(file_name) = mp_path.file_name().and_then(|n| n.to_str()) {
                                                                     self.migration_display_version = extract_version_from_filename(file_name);
                                                                 }
                                                             }
                                                         }
                                                     }
                                                 }
                                                 if ui.button("📁").on_hover_text("Вибрати файл збірки (.mrpack, .zip)").clicked() {
                                                     if let Some(path) = rfd::FileDialog::new()
                                                         .add_filter("Minecraft Modpacks", &["mrpack", "zip"])
                                                         .pick_file() {
                                                         self.migration_modpack_path = path.to_string_lossy().to_string();
                                                         if let Ok((ver, _loader)) = mod_manager::extract_modpack_version_and_loader(&path) {
                                                             self.migration_version_input = ver;
                                                             if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                                                                 self.migration_display_version = extract_version_from_filename(file_name);
                                                             }
                                                         }
                                                     }
                                                 }
                                             });
                                             ui.end_row();

                                             ui.label("Визначена версія Minecraft:");
                                             ui.label(&self.migration_version_input);
                                             ui.end_row();

                                             ui.label("Зберегти як версію в менеджері:");
                                             ui.text_edit_singleline(&mut self.migration_display_version)
                                                 .on_hover_text("Ця назва версії буде відображатись у списку серверів (наприклад, 26.2)");
                                             ui.end_row();
                                         });
                                 }

                                 ui.add_space(5.0);

                                 // Кнопка запуску міграції
                                 let can_migrate = if self.migration_use_modpack {
                                     !self.migration_modpack_path.trim().is_empty() && std::path::Path::new(self.migration_modpack_path.trim()).exists() && !self.migration_version_input.trim().is_empty()
                                 } else {
                                     !self.migration_version_input.trim().is_empty()
                                 };

                                 if ui.add_enabled(!self.migration_is_running && can_migrate, egui::Button::new("🚀 Запустити міграцію сервера")).clicked() {
                                     let srv_name = self.servers[selected_idx].name.clone();
                                     let srv_path = self.servers[selected_idx].path.clone();
                                     let current_loader = self.servers[selected_idx].loader_type.clone();

                                     let (target_version, target_loader, target_display, modpack_path) = if self.migration_use_modpack {
                                         let mp_path = std::path::PathBuf::from(self.migration_modpack_path.trim());
                                         let (_, loader) = mod_manager::extract_modpack_version_and_loader(&mp_path).unwrap_or((self.migration_version_input.clone(), current_loader));
                                         let display = if self.migration_display_version.trim().is_empty() {
                                             self.migration_version_input.clone()
                                         } else {
                                             self.migration_display_version.trim().to_string()
                                         };
                                         (self.migration_version_input.trim().to_string(), loader, display, Some(mp_path))
                                     } else {
                                         (self.migration_version_input.trim().to_string(), current_loader, self.migration_version_input.trim().to_string(), None)
                                     };

                                     let logs_tx = self.migration_logs_sender.clone();
                                     let (finish_tx, finish_rx) = std::sync::mpsc::channel();
                                     let progress = self.migration_progress.clone();
                                     let cf_key = self.config.curseforge_key.clone();

                                     self.migration_is_running = true;
                                     self.migration_progress.store(0, std::sync::atomic::Ordering::Relaxed);
                                     self.migration_log_buffer = format!("[Migration] Початок міграції на версію {}...\n", target_display);
                                     self.migration_finish_receiver = Some(finish_rx);
                                     let ctx_clone = ctx.clone();

                                     std::thread::spawn(move || {
                                         let res = crate::server_manager::migrate_server(
                                             &srv_name,
                                             &srv_path,
                                             &target_loader,
                                             &target_version,
                                             modpack_path.as_deref(),
                                             &cf_key,
                                             logs_tx,
                                             progress,
                                         );
                                         match res {
                                             Ok(_) => {
                                                 let _ = finish_tx.send(Ok((target_display, target_loader)));
                                             }
                                             Err(e) => {
                                                 let _ = finish_tx.send(Err(e));
                                             }
                                         }
                                         ctx_clone.request_repaint();
                                     });
                                 }

                                 // Показуємо прогрес-бар
                                 if self.migration_is_running {
                                     let val = self.migration_progress.load(std::sync::atomic::Ordering::Relaxed) as f32 / 100.0;
                                     ui.add_space(5.0);
                                     ui.add(egui::ProgressBar::new(val).show_percentage().text("Виконання..."));
                                 }

                                 if self.migration_is_running || !self.migration_log_buffer.is_empty() {
                                     ui.add_space(5.0);
                                     ui.label("Лог процесу:");
                                     egui::ScrollArea::vertical()
                                         .max_height(120.0)
                                         .stick_to_bottom(true)
                                         .show(ui, |ui| {
                                             ui.add(
                                                 egui::TextEdit::multiline(&mut self.migration_log_buffer)
                                                     .font(egui::TextStyle::Monospace)
                                                     .desired_width(f32::INFINITY)
                                             );
                                         });
                                 }
                             });

                            // Секція Скидання світу
                            ui.add_space(15.0);
                            ui.heading("🔄 Скидання світу та гравців");
                            ui.separator();
                            ui.add_space(5.0);
                            
                            ui.group(|ui| {
                                ui.label("Очищення всіх файлів світу, білих списків та авторизації.");
                                ui.horizontal(|ui| {
                                    ui.label("Сід світу (Seed):");
                                    ui.text_edit_singleline(&mut self.reset_seed);
                                });
                                ui.checkbox(&mut self.reset_hardcore, "Увімкнути Hardcore режим (складність Hard)");
                                
                                ui.add_space(5.0);
                                if ui.button("🔥 Очистити та перегенерувати світ").clicked() {
                                    match self.servers[selected_idx].reset_world_files(&self.reset_seed, self.reset_hardcore) {
                                        Ok(_) => {
                                            self.reset_success_msg = Some("Світ успішно скинуто! При наступному запуску сервер згенерує новий світ.".to_string());
                                            self.reset_error_msg = None;
                                        }
                                        Err(e) => {
                                            self.reset_error_msg = Some(format!("Помилка скидання світу: {}", e));
                                            self.reset_success_msg = None;
                                        }
                                    }
                                }
                                
                                if let Some(ref msg) = self.reset_success_msg {
                                    ui.colored_label(egui::Color32::from_rgb(50, 180, 50), msg);
                                }
                                if let Some(ref err) = self.reset_error_msg {
                                    ui.colored_label(egui::Color32::from_rgb(220, 50, 50), err);
                                }
                            });
                        } else {
                            ui.colored_label(
                                egui::Color32::from_rgb(150, 150, 150),
                                "Для зміни налаштувань та скидання світу спочатку зупиніть сервер."
                            );
                        }
                    }
                    1 => {
                        // ВКЛАДКА WHITELIST
                        self.sync_whitelist_cache(false);
                        
                        ui.add_space(5.0);
                        ui.horizontal(|ui| {
                            ui.label("Нікнейм гравця:");
                            ui.text_edit_singleline(&mut self.whitelist_input);
                            
                            if ui.button("➕ Додати до Whitelist").clicked() {
                                let name = self.whitelist_input.trim().to_string();
                                if !name.is_empty() {
                                    let srv = &mut self.servers[selected_idx];
                                    let _ = whitelist::add_to_whitelist(&srv.path, &name);
                                    if srv.status != ServerStatus::Offline {
                                        if let Some(ref mut child) = srv.child_process {
                                            if let Some(ref mut stdin) = child.stdin {
                                                let _ = writeln!(stdin, "whitelist add {}", name);
                                                let _ = stdin.flush();
                                            }
                                        }
                                    }
                                    self.whitelist_input.clear();
                                    self.whitelist_cache = whitelist::load_whitelist(&srv.path);
                                }
                            }
                        });

                        ui.add_space(10.0);
                        ui.separator();
                        ui.add_space(5.0);

                        ui.label("Список гравців у білому списку:");
                        
                        let mut to_remove = None;
                        egui::ScrollArea::vertical().max_height(ui.available_height() - 40.0).show(ui, |ui| {
                            for entry in &self.whitelist_cache {
                                ui.horizontal(|ui| {
                                    ui.label(format!("• {} ({})", entry.name, entry.uuid));
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if ui.button("❌ Видалити").clicked() {
                                            to_remove = Some(entry.name.clone());
                                        }
                                    });
                                });
                            }
                        });

                        if let Some(name) = to_remove {
                            let srv = &mut self.servers[selected_idx];
                            let _ = whitelist::remove_from_whitelist(&srv.path, &name);
                            if srv.status != ServerStatus::Offline {
                                if let Some(ref mut child) = srv.child_process {
                                    if let Some(ref mut stdin) = child.stdin {
                                        let _ = writeln!(stdin, "whitelist remove {}", name);
                                        let _ = stdin.flush();
                                    }
                                }
                            }
                            self.whitelist_cache = whitelist::load_whitelist(&srv.path);
                        }
                    }
                    2 => {
                        // ВКЛАДКА КОНСОЛЬ
                        ui.add_space(5.0);

                        let text_style = egui::TextStyle::Monospace;
                        let row_height = ui.text_style_height(&text_style);
                        
                        let srv = &mut self.servers[selected_idx];
                        
                        egui::ScrollArea::vertical()
                            .max_height(ui.available_height() - 55.0)
                            .stick_to_bottom(true)
                            .show_rows(ui, row_height, srv.log_buffer.lines().count(), |ui, row_range| {
                                let lines: Vec<&str> = srv.log_buffer.lines().collect();
                                let display_text = lines[row_range.start..row_range.end].join("\n");
                                ui.add(
                                    egui::TextEdit::multiline(&mut display_text.clone())
                                        .font(egui::FontId::monospace(10.5))
                                        .lock_focus(true)
                                        .desired_width(ui.available_width())
                                        .desired_rows(18)
                                );
                            });

                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.label(">");
                            let text_edit = ui.text_edit_singleline(&mut self.console_input);
                            
                            let mut send_command = false;
                            if text_edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                send_command = true;
                            }
                            
                            if ui.button("Надіслати").clicked() {
                                send_command = true;
                            }

                            if send_command {
                                let cmd = self.console_input.trim().to_string();
                                if !cmd.is_empty() {
                                    srv.append_log(format!("> {}", cmd));
                                    if let Some(ref mut child) = srv.child_process {
                                        if let Some(ref mut stdin) = child.stdin {
                                            let _ = writeln!(stdin, "{}", cmd);
                                            let _ = stdin.flush();
                                        }
                                    } else {
                                        srv.append_log("[GUI ERROR] Сервер не запущено, не вдалося відправити команду.".to_string());
                                    }
                                }
                                self.console_input.clear();
                                text_edit.request_focus();
                            }
                        });
                    }
                    3 => {
                        // ВКЛАДКА МОДИ
                        ui.add_space(5.0);
                        ui.horizontal(|ui| {
                            ui.selectable_value(&mut self.active_mods_tab, 0, "📂 Встановлені");
                            ui.selectable_value(&mut self.active_mods_tab, 1, "🔍 Modrinth Моди");
                            ui.selectable_value(&mut self.active_mods_tab, 2, "🔍 CurseForge Моди");
                            ui.selectable_value(&mut self.active_mods_tab, 3, "📦 Мод-збірки");
                        });
                        ui.separator();

                        match self.active_mods_tab {
                            0 => {
                                // 1. Список встановлених та авто-оновлення
                                let server_path = self.servers[selected_idx].path.clone();
                                let mc_ver = self.servers[selected_idx].version.clone();
                                let loader = self.servers[selected_idx].loader_type.clone();
                                let cf_key = self.config.curseforge_key.clone();

                                ui.horizontal(|ui| {
                                    if ui.add_enabled(!self.auto_update_is_running, egui::Button::new("🔄 Авто-оновлення модів")).clicked() {
                                        self.auto_update_is_running = true;
                                        self.auto_update_log_buffer.clear();
                                        
                                        let s_path = server_path.clone();
                                        let version = mc_ver.clone();
                                        let loader = loader.clone();
                                        let key = cf_key.clone();
                                        let tx_log = self.auto_update_logs_sender.clone();
                                        let (tx_fin, rx_fin) = channel();
                                        self.auto_update_finish_receiver = Some(rx_fin);
                                        let ctx_clone = ctx.clone();
                                        
                                        thread::spawn(move || {
                                            let res = mod_manager::check_and_update_mods(&s_path, &version, &loader, &key, &tx_log);
                                            let _ = tx_fin.send(res);
                                            ctx_clone.request_repaint();
                                        });
                                    }
                                });

                                if self.auto_update_is_running {
                                    ui.add_space(5.0);
                                    ui.horizontal(|ui| {
                                        ui.spinner();
                                        ui.label("Перевірка та завантаження оновлень...");
                                    });
                                    ui.add_space(5.0);
                                    
                                    let text_style = egui::TextStyle::Monospace;
                                    let row_height = ui.text_style_height(&text_style);
                                    egui::ScrollArea::vertical()
                                        .max_height(120.0)
                                        .stick_to_bottom(true)
                                        .show_rows(ui, row_height, self.auto_update_log_buffer.lines().count(), |ui, row_range| {
                                            let lines: Vec<&str> = self.auto_update_log_buffer.lines().collect();
                                            let display_text = lines[row_range.start..row_range.end].join("\n");
                                            ui.add(
                                                egui::TextEdit::multiline(&mut display_text.clone())
                                                    .font(egui::FontId::monospace(9.5))
                                                    .desired_width(ui.available_width())
                                                    .desired_rows(6)
                                            );
                                        });
                                    ui.separator();
                                }

                                self.sync_installed_mods_cache(false);
                                if self.installed_mods.is_empty() {
                                    ui.label("У папці mods/ немає встановлених файлів.");
                                } else {
                                    egui::ScrollArea::vertical().max_height(ui.available_height() - 150.0).show(ui, |ui| {
                                        let mut to_delete = None;
                                        for m in &self.installed_mods {
                                            ui.group(|ui| {
                                                ui.horizontal(|ui| {
                                                    if let Some(ref icon) = m.icon_url {
                                                        ui.add(egui::Image::new(icon).max_width(64.0).max_height(64.0));
                                                    } else {
                                                        ui.label("🔌");
                                                    }
                                                    ui.vertical(|ui| {
                                                        ui.heading(&m.filename);
                                                    });
                                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                        if ui.button("🗑").on_hover_text("Видалити цей мод").clicked() {
                                                            to_delete = Some(m.filename.clone());
                                                        }
                                                    });
                                                });
                                            });
                                        }
                                        if let Some(filename) = to_delete {
                                            let _ = mod_manager::delete_mod(&server_path, &filename);
                                            self.sync_installed_mods_cache(true);
                                        }
                                    });
                                }
                            }
                            1 => {
                                // 2. Пошук Modrinth
                                ui.horizontal(|ui| {
                                    ui.label("Пошук мода:");
                                    let text_edit = ui.text_edit_singleline(&mut self.mod_search_query);
                                    
                                    let mut search_clicked = false;
                                    if text_edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                        search_clicked = true;
                                    }
                                    if ui.button("🔍 Пошук").clicked() {
                                        search_clicked = true;
                                    }

                                    if search_clicked && !self.mod_search_query.trim().is_empty() {
                                        self.mod_search_loading = true;
                                        self.mod_search_error = None;
                                        self.mod_search_results.clear();
                                        
                                        let query = self.mod_search_query.clone();
                                        let (tx, rx) = channel();
                                        self.search_receiver = Some(rx);
                                        let ctx_clone = ctx.clone();
                                        thread::spawn(move || {
                                            let res = mod_manager::search_modrinth_projects(&query);
                                            let _ = tx.send(res);
                                            ctx_clone.request_repaint();
                                        });
                                    }
                                });

                                if self.mod_search_loading {
                                    ui.horizontal(|ui| {
                                        ui.spinner();
                                        ui.label("Пошук модів на Modrinth...");
                                    });
                                }

                                if let Some(ref err) = self.mod_search_error {
                                    ui.colored_label(egui::Color32::from_rgb(220, 50, 50), err);
                                }

                                if let Some(ref status) = self.download_status {
                                    ui.colored_label(egui::Color32::from_rgb(50, 180, 50), status);
                                }

                                ui.separator();
                                ui.label("Результати пошуку Modrinth:");

                                let mc_ver = self.servers[selected_idx].version.clone();
                                let server_path = self.servers[selected_idx].path.clone();
                                let loader = self.servers[selected_idx].loader_type.clone();

                                egui::ScrollArea::vertical().max_height(ui.available_height() - 100.0).show(ui, |ui| {
                                    for hit in &self.mod_search_results {
                                        ui.group(|ui| {
                                            ui.horizontal(|ui| {
                                                // Відображаємо іконку мода
                                                if let Some(ref icon_url) = hit.icon_url {
                                                    ui.add(egui::Image::new(icon_url).max_width(64.0).max_height(64.0));
                                                } else {
                                                    ui.label("🔌");
                                                }
                                                
                                                ui.vertical(|ui| {
                                                    ui.horizontal(|ui| {
                                                        ui.heading(&hit.title);
                                                        
                                                        if hit.server_side == "unsupported" {
                                                            ui.colored_label(egui::Color32::from_rgb(220, 50, 50), "[Клієнтський]");
                                                        } else {
                                                            ui.colored_label(egui::Color32::from_rgb(50, 180, 50), "[Серверний]");
                                                        }
                                                    });
                                                    ui.label(&hit.description);
                                                    
                                                    let is_client_only = hit.server_side == "unsupported";
                                                    let is_downloading = self.download_receiver.is_some();

                                                    let btn_label = if is_client_only {
                                                        "🚫 Клієнт-only (не для сервера)"
                                                    } else {
                                                        "📥 Встановити на сервер"
                                                    };
                                                    
                                                    if ui.add_enabled(!is_client_only && !is_downloading, egui::Button::new(btn_label)).clicked() {
                                                        self.download_status = Some("Пошук сумісних версій...".to_string());
                                                        let project_id = hit.project_id.clone();
                                                        let icon_url = hit.icon_url.clone();
                                                        let mc_version = mc_ver.clone();
                                                        let srv_path = server_path.clone();
                                                        let loader = loader.clone();
                                                        let (tx, rx) = channel();
                                                        self.download_receiver = Some(rx);
                                                        let ctx_clone = ctx.clone();
                                                        
                                                        thread::spawn(move || {
                                                            let run = move || -> Result<(), String> {
                                                                let versions = mod_manager::fetch_project_versions(&project_id, &mc_version, &loader)?;
                                                                if versions.is_empty() {
                                                                    return Err(format!("Не знайдено сумісних версій {} для Minecraft {}", loader, mc_version));
                                                                }
                                                                let v = &versions[0];
                                                                if v.files.is_empty() {
                                                                    return Err("У знайденої версії немає файлів для завантаження".to_string());
                                                                }
                                                                let file = v.files.iter().find(|f| f.primary).unwrap_or(&v.files[0]);
                                                                mod_manager::download_mod_file(&file.url, &srv_path, &file.filename)?;
                                                                
                                                                // Записуємо у метадані для авто-оновлень
                                                                mod_manager::add_installed_mod_metadata(&srv_path, mod_manager::InstalledModMetadata {
                                                                    filename: file.filename.clone(),
                                                                    source: "modrinth".to_string(),
                                                                    project_id: project_id.clone(),
                                                                    version_id: v.id.clone(),
                                                                    icon_url,
                                                                });
                                                                Ok(())
                                                            };
                                                            let _ = tx.send(run());
                                                            ctx_clone.request_repaint();
                                                        });
                                                    }
                                                });
                                            });
                                        });
                                    }
                                });
                            }
                            2 => {
                                // 3. Пошук CurseForge
                                ui.horizontal(|ui| {
                                    ui.label("CurseForge API Key:");
                                    let mut key = self.config.curseforge_key.clone();
                                    if ui.text_edit_singleline(&mut key).changed() {
                                        self.config.curseforge_key = key;
                                        self.save_app_config();
                                    }
                                    ui.label("(якщо пусто - використовується публічний проксі)");
                                });
                                ui.add_space(5.0);

                                ui.horizontal(|ui| {
                                    ui.label("Пошук мода:");
                                    let text_edit = ui.text_edit_singleline(&mut self.cf_search_query);
                                    
                                    let mut search_clicked = false;
                                    if text_edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                        search_clicked = true;
                                    }
                                    if ui.button("🔍 Пошук").clicked() {
                                        search_clicked = true;
                                    }

                                    if search_clicked && !self.cf_search_query.trim().is_empty() {
                                        self.cf_search_loading = true;
                                        self.cf_search_error = None;
                                        self.cf_search_results.clear();
                                        
                                        let query = self.cf_search_query.clone();
                                        let key = self.config.curseforge_key.clone();
                                        let loader = self.servers[selected_idx].loader_type.clone();
                                        let (tx, rx) = channel();
                                        self.cf_search_receiver = Some(rx);
                                        let ctx_clone = ctx.clone();
                                        thread::spawn(move || {
                                            let res = mod_manager::search_curseforge_projects(&query, &loader, &key);
                                            let _ = tx.send(res);
                                            ctx_clone.request_repaint();
                                        });
                                    }
                                });

                                if self.cf_search_loading {
                                    ui.horizontal(|ui| {
                                        ui.spinner();
                                        ui.label("Пошук модів на CurseForge...");
                                    });
                                }

                                if let Some(ref err) = self.cf_search_error {
                                    ui.colored_label(egui::Color32::from_rgb(220, 50, 50), err);
                                }

                                if let Some(ref status) = self.download_status {
                                    ui.colored_label(egui::Color32::from_rgb(50, 180, 50), status);
                                }

                                ui.separator();
                                ui.label("Результати пошуку CurseForge:");

                                let mc_ver = self.servers[selected_idx].version.clone();
                                let server_path = self.servers[selected_idx].path.clone();
                                let cf_key = self.config.curseforge_key.clone();
                                let current_loader = self.servers[selected_idx].loader_type.clone();

                                egui::ScrollArea::vertical().max_height(ui.available_height() - 130.0).show(ui, |ui| {
                                    for hit in &self.cf_search_results {
                                        ui.group(|ui| {
                                            ui.horizontal(|ui| {
                                                // Відображаємо іконку CurseForge мода
                                                if let Some(ref logo) = hit.logo {
                                                    ui.add(egui::Image::new(&logo.thumbnail_url).max_width(64.0).max_height(64.0));
                                                } else {
                                                    ui.label("🔌");
                                                }
                                                
                                                ui.vertical(|ui| {
                                                    ui.heading(&hit.name);
                                                    ui.label(&hit.summary);
                                                    
                                                    // Фільтруємо сумісні файли за версією та завантажувачем
                                                    let cf_loader_id = match current_loader.as_str() {
                                                        "forge" => Some(1),
                                                        "neoforge" => Some(6),
                                                        "fabric" => Some(4),
                                                        _ => None,
                                                    };
                                                    
                                                    let mut compatible_files: Vec<mod_manager::CurseForgeFile> = hit.latest_files.iter()
                                                        .filter(|f| {
                                                            let supports_mc = f.game_versions.iter().any(|gv| *gv == mc_ver);
                                                            let supports_loader = cf_loader_id.is_none() || f.mod_loader_type == cf_loader_id;
                                                            supports_mc && supports_loader
                                                        })
                                                        .cloned()
                                                        .collect();
                                                    
                                                    compatible_files.sort_by(|a, b| b.id.cmp(&a.id));
                                                    
                                                    let has_file = !compatible_files.is_empty();
                                                    let is_downloading = self.download_receiver.is_some();

                                                    let btn_label = if has_file {
                                                        "📥 Встановити на сервер"
                                                    } else {
                                                        "🚫 Немає сумісного файлу"
                                                    };

                                                    if ui.add_enabled(has_file && !is_downloading, egui::Button::new(btn_label)).clicked() {
                                                        self.download_status = Some("Завантаження з CurseForge...".to_string());
                                                        let mod_id = hit.id;
                                                        let file = compatible_files[0].clone();
                                                        let srv_path = server_path.clone();
                                                        let key = cf_key.clone();
                                                        let (tx, rx) = channel();
                                                        self.download_receiver = Some(rx);
                                                        let ctx_clone = ctx.clone();

                                                        thread::spawn(move || {
                                                            let run = move || -> Result<(), String> {
                                                                let download_url = match file.download_url {
                                                                    Some(ref url) => url.clone(),
                                                                    None => {
                                                                        mod_manager::fetch_curseforge_download_url(mod_id, file.id, &key)?
                                                                    }
                                                                };
                                                                mod_manager::download_mod_file(&download_url, &srv_path, &file.file_name)?;
                                                                
                                                                // Записуємо у метадані для авто-оновлень
                                                                mod_manager::add_installed_mod_metadata(&srv_path, mod_manager::InstalledModMetadata {
                                                                    filename: file.file_name.clone(),
                                                                    source: "curseforge".to_string(),
                                                                    project_id: mod_id.to_string(),
                                                                    version_id: file.id.to_string(),
                                                                    icon_url: None,
                                                                });
                                                                Ok(())
                                                            };
                                                            let _ = tx.send(run());
                                                            ctx_clone.request_repaint();
                                                        });
                                                    }
                                                });
                                            });
                                        });
                                    }
                                });
                            }
                            3 => {
                                // 4. Встановлення збірки
                                ui.heading("Встановлення онлайн збірки з Modrinth:");
                                ui.horizontal(|ui| {
                                    ui.label("Пошук збірки:");
                                    let text_edit = ui.text_edit_singleline(&mut self.pack_search_query);
                                    
                                    let mut search_clicked = false;
                                    if text_edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                        search_clicked = true;
                                    }
                                    if ui.button("🔍 Пошук").clicked() {
                                        search_clicked = true;
                                    }

                                    if search_clicked && !self.pack_search_query.trim().is_empty() {
                                        self.pack_search_loading = true;
                                        self.pack_search_error = None;
                                        self.pack_search_results.clear();
                                        
                                        let query = self.pack_search_query.clone();
                                        let (tx, rx) = channel();
                                        self.pack_search_receiver = Some(rx);
                                        let ctx_clone = ctx.clone();
                                        thread::spawn(move || {
                                            let res = mod_manager::search_modrinth_modpacks(&query);
                                            let _ = tx.send(res);
                                            ctx_clone.request_repaint();
                                        });
                                    }
                                });

                                if self.pack_search_loading {
                                    ui.horizontal(|ui| {
                                        ui.spinner();
                                        ui.label("Пошук збірок на Modrinth...");
                                    });
                                }

                                if let Some(ref err) = self.pack_search_error {
                                    ui.colored_label(egui::Color32::from_rgb(220, 50, 50), err);
                                }

                                if let Some(ref status) = self.pack_download_status {
                                    ui.colored_label(egui::Color32::from_rgb(50, 180, 50), status);
                                }

                                let mc_ver = self.servers[selected_idx].version.clone();
                                let server_path = self.servers[selected_idx].path.clone();

                                egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                                    for hit in &self.pack_search_results {
                                        ui.group(|ui| {
                                            ui.horizontal(|ui| {
                                                if let Some(ref icon) = hit.icon_url {
                                                    ui.add(egui::Image::new(icon).max_width(40.0).max_height(40.0));
                                                }
                                                ui.vertical(|ui| {
                                                    ui.heading(&hit.title);
                                                    ui.label(&hit.description);
                                                    
                                                    let is_installing = self.pack_download_receiver.is_some();
                                                    if ui.add_enabled(!is_installing, egui::Button::new("📥 Скачати та встановити")).clicked() {
                                                        self.pack_download_status = Some("Запит версій збірки...".to_string());
                                                        self.modpack_is_installing = true;
                                                        self.modpack_log_buffer.clear();
                                                        
                                                        let project_id = hit.project_id.clone();
                                                        let mc_version = mc_ver.clone();
                                                        let srv_path = server_path.clone();
                                                        let log_tx = self.modpack_logs_sender.clone();
                                                        let loader = self.servers[selected_idx].loader_type.clone();
                                                        let cf_key = self.config.curseforge_key.clone();
                                                        let (tx, rx) = channel();
                                                        self.pack_download_receiver = Some(rx);
                                                        let ctx_clone = ctx.clone();

                                                        thread::spawn(move || {
                                                            let run = move || -> Result<(), String> {
                                                                let versions = mod_manager::fetch_project_versions(&project_id, &mc_version, &loader)?;
                                                                if versions.is_empty() {
                                                                    return Err(format!("Не знайдено версій збірки для Minecraft {}", mc_version));
                                                                }
                                                                let v = &versions[0];
                                                                if v.files.is_empty() {
                                                                    return Err("Немає доступних файлів у збірці".to_string());
                                                                }
                                                                let file = &v.files[0];
                                                                let _ = log_tx.send(format!("[Modpack] Завантаження mrpack файлу збірки: {}", file.filename));
                                                                
                                                                let temp_path = srv_path.join("temp_modpack.mrpack");
                                                                mod_manager::download_mod_file(&file.url, &srv_path, "temp_modpack.mrpack")?;
                                                                
                                                                let res = mod_manager::install_mrpack(&temp_path, &srv_path, &cf_key, &log_tx);
                                                                let _ = std::fs::remove_file(temp_path);
                                                                res
                                                            };
                                                            let _ = tx.send(run());
                                                            ctx_clone.request_repaint();
                                                        });
                                                    }
                                                });
                                            });
                                        });
                                    }
                                });

                                ui.separator();
                                ui.heading("Встановлення локальної збірки:");
                                ui.label("Введіть абсолютний шлях до локального файлу збірки .mrpack:");
                                ui.text_edit_singleline(&mut self.modpack_path_input);
                                ui.add_space(5.0);

                                let is_installing = self.modpack_is_installing;
                                if is_installing {
                                    ui.horizontal(|ui| {
                                        ui.spinner();
                                        ui.label("Встановлення збірки (завантаження модів та конфігів)...");
                                    });
                                } else {
                                    let path_str = self.modpack_path_input.trim().to_string();
                                    
                                    if ui.add_enabled(!path_str.is_empty(), egui::Button::new("📦 Встановити локальну збірку")).clicked() {
                                        self.modpack_is_installing = true;
                                        self.modpack_log_buffer.clear();
                                        
                                        let mrpack_path = PathBuf::from(&path_str);
                                        let log_tx = self.modpack_logs_sender.clone();
                                        let (tx_fin, rx_fin) = channel();
                                        self.modpack_finish_receiver = Some(rx_fin);
                                        let ctx_clone = ctx.clone();
                                        
                                        let cf_key = self.config.curseforge_key.clone();
                                        thread::spawn(move || {
                                            let res = mod_manager::install_mrpack(&mrpack_path, &server_path, &cf_key, &log_tx);
                                            let _ = tx_fin.send(res);
                                            ctx_clone.request_repaint();
                                        });
                                    }
                                }

                                ui.add_space(10.0);
                                ui.label("Лог процесу встановлення збірки:");
                                
                                let text_style = egui::TextStyle::Monospace;
                                let row_height = ui.text_style_height(&text_style);
                                egui::ScrollArea::vertical()
                                    .max_height(200.0)
                                    .stick_to_bottom(true)
                                    .show_rows(ui, row_height, self.modpack_log_buffer.lines().count(), |ui, row_range| {
                                        let lines: Vec<&str> = self.modpack_log_buffer.lines().collect();
                                        let display_text = lines[row_range.start..row_range.end].join("\n");
                                        ui.add(
                                            egui::TextEdit::multiline(&mut display_text.clone())
                                                .font(egui::FontId::monospace(9.5))
                                                .desired_width(ui.available_width())
                                                .desired_rows(10)
                                        );
                                    });
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            } else {
                // СТАРТОВИЙ ЕКРАН ЯКЩО СЕРВЕРІВ НЕМАЄ
                ui.vertical_centered(|ui| {
                    ui.add_space(50.0);
                    ui.heading("Вітаємо у Minecraft Server Manager!");
                    ui.label("У вас ще немає доданих серверів.");
                    ui.add_space(20.0);
                    ui.label("Натисніть кнопку '➕ Створити сервер' зліва, щоб створити свій перший сервер.");
                });
            }
        });
    }

    // Безпечне вимкнення при закритті вікна
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        for server in &mut self.servers {
            server.stop();
            // Чекаємо завершення роботи сервера до 5 секунд
            if let Some(ref mut child) = server.child_process {
                let start_wait = Instant::now();
                while start_wait.elapsed() < Duration::from_secs(5) {
                    if let Ok(Some(_)) = child.try_wait() {
                        break;
                    }
                    thread::sleep(Duration::from_millis(50));
                }
            }
        }
    }
}

fn parse_version(s: &str) -> Vec<u32> {
    s.split(|c: char| !c.is_numeric())
     .filter_map(|p| p.parse::<u32>().ok())
     .collect()
}

pub fn is_version_newer(current: &str, candidate: &str) -> bool {
    let cur_parts = parse_version(current);
    let cand_parts = parse_version(candidate);
    if cur_parts.is_empty() || cand_parts.is_empty() {
        return true;
    }
    if cur_parts[0] != cand_parts[0] {
        return true;
    }
    cand_parts > cur_parts
}

fn extract_version_from_filename(filename: &str) -> String {
    let mut best_match = String::new();
    let chars: Vec<char> = filename.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_numeric() {
            let start = i;
            let mut dot_count = 0;
            while i < chars.len() && (chars[i].is_numeric() || chars[i] == '.') {
                if chars[i] == '.' {
                    dot_count += 1;
                }
                i += 1;
            }
            let candidate: String = chars[start..i].iter().collect();
            let cleaned = candidate.trim_matches('.');
            if dot_count >= 1 && cleaned.chars().next().map_or(false, |c| c.is_numeric()) {
                if cleaned.len() > best_match.len() {
                    best_match = cleaned.to_string();
                }
            }
        } else {
            i += 1;
        }
    }
    if best_match.is_empty() {
        filename.to_string()
    } else {
        best_match
    }
}
