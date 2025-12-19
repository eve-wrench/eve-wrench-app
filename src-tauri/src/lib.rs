mod esi;
mod evesettings;
mod updates;

use evesettings::{
    copy_settings, create_backup, delete_backup, get_app_data, set_alias, set_brackets_always_show,
};
use updates::check_for_update;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_os::init())
        .invoke_handler(tauri::generate_handler![
            get_app_data,
            create_backup,
            delete_backup,
            copy_settings,
            set_alias,
            set_brackets_always_show,
            check_for_update,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
