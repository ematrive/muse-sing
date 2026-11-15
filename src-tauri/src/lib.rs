pub mod audio;
pub mod commands;
pub mod corpus;
pub mod ds;
pub mod error;
pub mod lyrics;
pub mod mix;
pub mod score;
pub mod sing;
pub mod singers;
pub mod ustx;
pub mod voice;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            use tauri::Manager;
            app.manage(commands::AudioState::new(app.handle().clone()));
            app.manage(commands::SingState::new(app.handle()));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::analyze,
            commands::list_singers,
            commands::generate,
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
            commands::sing_play,
            commands::sing_stop,
            commands::sing_export,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
