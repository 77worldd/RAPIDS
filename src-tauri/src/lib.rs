mod adapter;
mod commands;
mod platform;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(adapter::setup)
        .invoke_handler(tauri::generate_handler![
            commands::app_version,
            commands::tasks_list,
            commands::task_add,
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
