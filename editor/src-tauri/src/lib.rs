pub mod byte_char_mapper;
pub mod commands;
pub mod manager;
pub mod bin;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_window_state::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_tokens,
            commands::file_update,
            commands::file_close,
            commands::get_diagnostics,
            commands::play_note,
            commands::get_events,
            commands::set_volume,
            commands::get_volume,
            commands::validate_midi_export,
            commands::export_midi,
            commands::start_lsp_server,
            commands::stop_lsp_server,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
