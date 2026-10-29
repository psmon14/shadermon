use dirs::home_dir;
use notify_rust::Notification;
use regex::Regex;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tray_icon::{
    Icon, TrayIconBuilder,
    menu::{Menu, MenuItem},
};

const ICON_BYTES: &[u8] = include_bytes!("../assets/icon.png");

const IDLE_TIMEOUT: Duration = Duration::from_secs(12);

#[derive(Debug, Clone)]
struct ShaderProgress {
    app_name: String,
    percent_num: u32,
    percent_str: String,
    compiled: String,
    total: String,
    last_seen: Instant,
}

#[derive(Default)]
struct ShaderQueue {
    order: Vec<String>,
    entries: HashMap<String, ShaderProgress>,
}

impl ShaderQueue {
    fn upsert(&mut self, app_id: String, progress: ShaderProgress) {
        if !self.entries.contains_key(&app_id) {
            self.order.push(app_id.clone());
        }
        self.entries.insert(app_id, progress);
    }

    fn complete_highest(&mut self) -> Option<(String, String)> {
        let app_id = self
            .entries
            .iter()
            .max_by_key(|(_, p)| p.percent_num)
            .map(|(id, _)| id.clone())?;

        let progress = self.entries.remove(&app_id)?;
        self.order.retain(|id| id != &app_id);
        Some((app_id, progress.app_name))
    }

    fn reap_idle(&mut self) -> Vec<(String, String)> {
        let now = Instant::now();
        let mut finished = Vec::new();
        self.order.retain(|app_id| {
            if let Some(p) = self.entries.get(app_id)
                && now.duration_since(p.last_seen) > IDLE_TIMEOUT
            {
                finished.push((app_id.clone(), p.app_name.clone()));
                self.entries.remove(app_id);
                return false;
            }
            true
        });
        finished
    }
}

fn notify_finished(app_name: &str) {
    let _ = Notification::new()
        .summary("Steam Shader Monitor")
        .body(&format!("Finished compiling shaders for:\n{}", app_name))
        .appname("shadermon")
        .icon("steam") //Should pull steam icon, i think?
        .timeout(Duration::from_secs(5))
        .show();
}

fn main() {
    if gtk::init().is_err() {
        eprintln!("Failed to initialize GTK.");
        return;
    }

    let home = home_dir().expect("Could not find home directory");
    let steam_base = home.join(".local/share/Steam");
    let log_path = steam_base.join("logs/shader_log.txt");
    let steamapps_path = steam_base.join("steamapps");
    let mut steamapps_dirs = vec![steamapps_path.clone()];

    if let Ok(extra_dirs) = find_steam_libraries(&steamapps_path) {
        steamapps_dirs.extend(extra_dirs);
    }

    let queue = Arc::new(Mutex::new(ShaderQueue::default()));
    let mut app_cache: HashMap<String, String> = HashMap::new();

    let tray_menu = Menu::new();
    let placeholder_item = MenuItem::new("No shaders compiling", false, None);
    let quit_item = MenuItem::new("Quit", true, None);
    let _ = tray_menu.append_items(&[&placeholder_item, &quit_item]);

    let icon = load_embedded_icon(ICON_BYTES).unwrap_or_else(|| {
        eprintln!("Warning: Failed to decode embedded icon. Falling back to blank canvas.");
        Icon::from_rgba(vec![128; 16 * 16 * 4], 16, 16).unwrap()
    });

    let mut tray_icon = Some(
        TrayIconBuilder::new()
            .with_menu(Box::new(tray_menu.clone()))
            .with_tooltip("Steam Shader Progress")
            .with_icon(icon)
            .build()
            .unwrap(),
    );

    let queue_bg = Arc::clone(&queue);
    let log_path_bg = log_path.clone();
    std::thread::spawn(move || {
        let progress_re = Regex::new(r"Still replaying (\d+)\s+\((\d+)%,\s+(\d+)/(\d+)\)").unwrap();
        let done_re = Regex::new(r"Destroyed compile job (\d+)").unwrap();

        let mut last_pos: u64 = std::fs::metadata(&log_path_bg)
            .map(|m| m.len())
            .unwrap_or(0);

        loop {
            if let Ok(mut file) = File::open(&log_path_bg)
                && let Ok(metadata) = file.metadata()
            {
                let len = metadata.len();

                if len < last_pos {
                    last_pos = 0;
                }

                if len > last_pos {
                    if file.seek(SeekFrom::Start(last_pos)).is_ok() {
                        let reader = BufReader::new(&file);
                        for line in reader.lines().map_while(Result::ok) {
                            if let Some(caps) = progress_re.captures(&line) {
                                let app_id = caps.get(1).unwrap().as_str().to_string();
                                let percent_raw = caps.get(2).unwrap().as_str();
                                let compiled = caps.get(3).unwrap().as_str().to_string();
                                let total = caps.get(4).unwrap().as_str().to_string();
                                let percent_num = percent_raw.parse::<u32>().unwrap_or(0);
                                let percent_str = format!("{}%", percent_raw);

                                let app_name = app_cache
                                    .entry(app_id.clone())
                                    .or_insert_with(|| resolve_app_name(&steamapps_dirs, &app_id))
                                    .clone();

                                let mut lock = queue_bg.lock().unwrap();
                                lock.upsert(
                                    app_id,
                                    ShaderProgress {
                                        app_name,
                                        percent_num,
                                        percent_str,
                                        compiled,
                                        total,
                                        last_seen: Instant::now(),
                                    },
                                );
                            } else if done_re.is_match(&line) {
                                let mut lock = queue_bg.lock().unwrap();
                                if let Some((_, app_name)) = lock.complete_highest() {
                                    drop(lock);
                                    notify_finished(&app_name);
                                }
                            }
                        }
                    }
                    last_pos = len;
                }
            }

            let stalled = queue_bg.lock().unwrap().reap_idle();
            for (_, app_name) in stalled {
                notify_finished(&app_name);
            }

            std::thread::sleep(Duration::from_millis(300));
        }
    });

    let main_loop = glib::MainLoop::new(None, false);

    let mut menu_items: HashMap<String, MenuItem> = HashMap::new();
    let tray_menu_ui = tray_menu.clone();
    let queue_ui = Arc::clone(&queue);
    glib::timeout_add_local(Duration::from_millis(500), move || {
        if let Ok(lock) = queue_ui.lock() {
            let active_ids: std::collections::HashSet<&String> = lock.order.iter().collect();
            menu_items.retain(|app_id, item| {
                if !active_ids.contains(app_id) {
                    let _ = tray_menu_ui.remove(item);
                    false
                } else {
                    true
                }
            });

            for app_id in &lock.order {
                if let Some(progress) = lock.entries.get(app_id) {
                    let bar = make_progress_bar(progress.percent_num);
                    let text = format!(
                        "{} \n{} {} ( {}/{} )",
                        progress.app_name,
                        bar,
                        progress.percent_str,
                        progress.compiled,
                        progress.total
                    );

                    if let Some(item) = menu_items.get(app_id) {
                        item.set_text(text);
                    } else {
                        let item = MenuItem::new(text, false, None);
                        let _ = tray_menu_ui.insert(&item, menu_items.len());
                        menu_items.insert(app_id.clone(), item);
                    }
                }
            }

            placeholder_item.set_text(if lock.order.is_empty() {
                "No shaders compiling".to_string()
            } else {
                format!("{} app(s) compiling shaders", lock.order.len())
            });
        }

        glib::ControlFlow::Continue
    });

    let main_loop_clone = main_loop.clone();
    glib::timeout_add_local(Duration::from_millis(100), move || {
        if let Ok(event) = tray_icon::menu::MenuEvent::receiver().try_recv()
            && event.id == quit_item.id()
        {
            let _ = tray_icon.take();
            main_loop_clone.quit();
        }

        glib::ControlFlow::Continue
    });

    main_loop.run();
}

fn make_progress_bar(percent: u32) -> String {
    let total_blocks: usize = 10;
    let filled_blocks = ((percent as f32 / 100.0) * total_blocks as f32).round() as usize;
    let empty_blocks = total_blocks.saturating_sub(filled_blocks);

    let filled = "█".repeat(filled_blocks);
    let empty = "░".repeat(empty_blocks);

    format!("[{}{}]", filled, empty)
}

fn load_embedded_icon(bytes: &[u8]) -> Option<Icon> {
    if let Ok(img) = image::load_from_memory(bytes) {
        let rgba = img.to_rgba8();
        let (width, height) = rgba.dimensions();
        let raw_pixels = rgba.into_raw();
        Icon::from_rgba(raw_pixels, width, height).ok()
    } else {
        None
    }
}

fn find_steam_libraries(default_steamapps: &Path) -> Result<Vec<PathBuf>, std::io::Error> {
    let mut dirs = Vec::new();
    let libraryfolders = default_steamapps.join("libraryfolders.vdf");
    let file = File::open(libraryfolders)?;
    let reader = BufReader::new(file);
    let path_re = Regex::new(r#""path"\s+"([^"]+)""#).unwrap();
    for line in reader.lines().map_while(Result::ok) {
        if let Some(caps) = path_re.captures(&line) {
            dirs.push(PathBuf::from(caps.get(1).unwrap().as_str()).join("steamapps"));
        }
    }
    Ok(dirs)
}

fn resolve_app_name(steamapps_dirs: &[PathBuf], app_id: &str) -> String {
    let name_re = Regex::new(r#""name"\s+"([^"]+)""#).unwrap();
    for steamapps in steamapps_dirs {
        let manifest_path = steamapps.join(format!("appmanifest_{}.acf", app_id));
        if let Ok(file) = File::open(manifest_path) {
            let reader = BufReader::new(file);
            for line in reader.lines().map_while(Result::ok) {
                if let Some(caps) = name_re.captures(&line) {
                    return caps.get(1).unwrap().as_str().to_string();
                }
            }
        }
    }
    format!("Unknown App ({})", app_id)
}
