pub mod accomp;
pub mod audio;
pub mod commands;
pub mod corpus;
pub mod ds;
pub mod edits;
pub mod error;
pub mod fx;
pub mod instruments;
pub mod latin;
pub mod lyrics;
pub mod marks;
pub mod mix;
pub mod musicxml;
pub mod project;
pub mod score;
pub mod sing;
pub mod singers;
pub mod tiger;
pub mod ustx;
pub mod voice;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        // managed before setup: on macOS the "open file" event can arrive first
        .manage(commands::OpenedFiles::default())
        .setup(|app| {
            use tauri::Manager;
            app.manage(commands::AudioState::new(app.handle().clone()));
            app.manage(commands::SingState::new(app.handle()));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::analyze,
            commands::list_singers,
            commands::install_tiger,
            commands::soundfont_installed,
            commands::install_soundfont,
            commands::voice_recent,
            commands::voice_create,
            commands::voice_open,
            commands::voice_add_piece,
            commands::voice_remove_group,
            commands::audio_guide,
            commands::audio_record,
            commands::audio_play_take,
            commands::audio_stop,
            commands::audio_devices,
            commands::audio_open,
            commands::audio_close,
            commands::audio_note,
            commands::sing_start,
            commands::sing_cancel,
            commands::sing_track_info,
            commands::sing_play,
            commands::sing_stop,
            commands::sing_export,
            commands::sing_export_guides,
            commands::project_new,
            commands::project_open,
            commands::project_save,
            commands::project_reload_score,
            commands::project_recent,
            commands::take_opened_files,
            commands::score_roll,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app, _event| {
            // double click on a project in the Finder (or "Open with"): macOS sends the files here
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            if let tauri::RunEvent::Opened { urls } = _event {
                let paths = urls.iter().filter_map(|u| u.to_file_path().ok()).map(|p| p.to_string_lossy().into_owned()).collect();
                commands::files_opened(_app, paths);
            }
        });
}
