mod caps;
mod commands;
mod error;
mod pipeline;
mod probe;
mod queue;
mod tools;
mod types;

use std::{
    collections::HashMap,
    fs,
    sync::{Arc, Mutex},
};
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let tools = tools::Tools::locate(app.path().resource_dir().ok());
            let q = Arc::new(queue::Queue::new());
            queue::spawn_worker(app.handle().clone(), q.clone(), tools.clone());
            app.manage(commands::AppState {
                queue: q,
                tools,
                caps: Mutex::new(None),
                bench: Mutex::new(HashMap::new()),
            });
            if let Ok(d) = app.path().app_cache_dir() {
                let _ = fs::remove_dir_all(d.join("frames")); // stale stills
                let _ = fs::remove_dir_all(d.join("previews"));
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_caps,
            commands::probe_file,
            commands::default_output,
            commands::estimate,
            commands::bench,
            commands::enqueue,
            commands::cancel_job,
            commands::list_jobs,
            commands::extract_frame,
            commands::reveal,
            commands::open_path,
            commands::get_prefs,
            commands::set_prefs,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Rex");
}
