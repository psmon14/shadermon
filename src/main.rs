use dirs::home_dir;
use notify_rust::Notification;
use regex::Regex;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tray_icon::{
    menu::{Menu, MenuItem},
    Icon, TrayIconBuilder,
};

const ICON_BYTES: &[u8] = include_bytes!("../assets/icon.png");

#[derive(Debug, Clone, Default)]
struct ShaderProgress {
    app_name: String,
    percent_num: u32,
    percent_str: String,
    compiled: String,
    total: String,
    is_active: bool,
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

    let current_progress = Arc::new(Mutex::new(ShaderProgress::default()));
    let mut app_cache: HashMap<String, String> = HashMap::new();

    let tray_menu = Menu::new();
    let progress_item = MenuItem::new("No shaders compiling", false, None);
    let quit_item = MenuItem::new("Quit", true, None);
    let _ = tray_menu.append_items(&[&progress_item, &quit_item]);

    let icon = load_embedded_icon(ICON_BYTES).unwrap_or_else(|| {
        eprintln!("Warning: Failed to decode embedded icon. Falling back to blank canvas.");
        Icon::from_rgba(vec![128; 16 * 16 * 4], 16, 16).unwrap()
    });
    
    let mut tray_icon = Some(
        TrayIconBuilder::new()
            .with_menu(Box::new(tray_menu))
            .with_tooltip("Steam Shader Progress")
            .with_icon(icon)
            .build()
            .unwrap(),
    );

    let progress_clone = Arc::clone(&current_progress);
    std::thread::spawn(move || {
        let progress_re = Regex::new(r"Still replaying (\d+)\s+\((\d+)%,\s+(\d+)/(\d+)\)").unwrap();
        let done_re = Regex::new(r"Destroyed compile job (\d+)").unwrap();
        let mut last_checked_len = 0;

        loop {
            if let Ok(file) = File::open(&log_path) {
                if let Ok(metadata) = file.metadata() {
                    let len = metadata.len();

                    if len != last_checked_len {
                        last_checked_len = len;
                        let reader = BufReader::new(file);
                        
                        if let Some(last_line) = reader.lines().filter_map(Result::ok).last() {
                            let mut lock = progress_clone.lock().unwrap();
                            
                            if let Some(caps) = progress_re.captures(&last_line) {
                                let app_id = caps.get(1).unwrap().as_str().to_string();
                                let percent_raw = caps.get(2).unwrap().as_str();
                                let compiled = caps.get(3).unwrap().as_str().to_string();
                                let total = caps.get(4).unwrap().as_str().to_string();

                                let percent_num = percent_raw.parse::<u32>().unwrap_or(0);
                                let percent_str = format!("{}%", percent_raw);

                                let app_name = app_cache.entry(app_id.clone()).or_insert_with(|| {
                                    resolve_app_name(&steamapps_path, &app_id)
                                }).clone();

                                *lock = ShaderProgress {
                                    app_name,
                                    percent_num,
                                    percent_str,
                                    compiled,
                                    total,
                                    is_active: true,
                                };
                            } else if let Some(done_caps) = done_re.captures(&last_line) {
                                let app_id = done_caps.get(1).unwrap().as_str().to_string();
                                let app_name = app_cache.entry(app_id.clone()).or_insert_with(|| {
                                    resolve_app_name(&steamapps_path, &app_id)
                                }).clone();
                                
                                if lock.is_active && !lock.app_name.is_empty() {
                                    let _ = Notification::new()
                                        .summary("Steam Shader Monitor")
                                        .body(&format!("Finished compiling shaders for:\n{}", lock.app_name))
                                        .appname("shadermon")
                                        .icon("steam") //Should pull steam icon, i think?
                                        .timeout(Duration::from_secs(5))
                                        .show();
                                }

                                *lock = ShaderProgress {
                                    is_active: false,
                                    ..Default::default()
                                };
                            }
                        }
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    });

    let main_loop = glib::MainLoop::new(None, false);

    let progress_ui_clone = Arc::clone(&current_progress);
    let progress_item_clone = progress_item.clone();
    glib::timeout_add_local(Duration::from_millis(500), move || {
        if let Ok(data) = progress_ui_clone.lock() {
            if data.is_active {
                let bar = make_progress_bar(data.percent_num);
                let text = format!(
                    "{} \n{} {} ( {}/{} )", 
                    data.app_name, bar, data.percent_str, data.compiled, data.total
                );
                progress_item_clone.set_text(text);
            } else {
                progress_item_clone.set_text("No active shader compilation");
            }
        }
        glib::ControlFlow::Continue
    });

    let main_loop_clone = main_loop.clone();
    glib::timeout_add_local(Duration::from_millis(100), move || {
        if let Ok(event) = tray_icon::menu::MenuEvent::receiver().try_recv() {
            if event.id == quit_item.id() {
                let _ = tray_icon.take(); 
                main_loop_clone.quit();
            }
        }
        glib::ControlFlow::Continue
    });

    main_loop.run();
}

fn make_progress_bar(percent: u32) -> String {
    let total_blocks = 10;
    let filled_blocks = ((percent as f32 / 100.0) * total_blocks as f32).round() as usize;
    let empty_blocks = total_blocks - filled_blocks;
    
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

fn resolve_app_name(steamapps: &PathBuf, app_id: &str) -> String {
    let manifest_path = steamapps.join(format!("appmanifest_{}.acf", app_id));
    if let Ok(file) = File::open(manifest_path) {
        let reader = BufReader::new(file);
        let name_re = Regex::new(r#""name"\s+"([^"]+)""#).unwrap();
        for line in reader.lines().filter_map(Result::ok) {
            if let Some(caps) = name_re.captures(&line) {
                return caps.get(1).unwrap().as_str().to_string();
            }
        }
    }
    format!("Unknown App ({})", app_id)
}
