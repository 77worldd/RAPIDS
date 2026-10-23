mod adapter;
mod commands;
mod deeplink;
mod platform;
mod sniff;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            deeplink::focus(app);
        }))
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            adapter::setup(app)?;
            deeplink::setup(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_version,
            deeplink::deeplink_take_pending,
            commands::download_default_dir,
            commands::open_download_file,
            commands::tasks_list,
            commands::task_add,
            commands::sniff_open,
            commands::task_pause,
            commands::task_resume,
            commands::task_retry,
            commands::task_cancel,
            commands::task_delete,
            commands::task_reorder,
            commands::queue_pause,
            commands::queue_resume,
            commands::queue_paused,
            commands::settings_get,
            commands::settings_update,
            commands::schedules_list,
            commands::schedule_create,
            commands::schedule_set_enabled,
            commands::schedule_delete,
            commands::queue_counts,
        ])
        .build(tauri::generate_context!())
        .expect("error while building RAPIDS")
        .run(|app, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                adapter::shutdown(app);
            }
        });
}
