//! Tauri command wrappers. The host decides what each operation does; this
//! module only resolves the machine's default folder and carries replies and
//! errors to the webview.

use std::path::PathBuf;

use rapids_host::api::{QueueCounts, ScheduleView, Settings};
use rapids_host::events::TaskView;
use rapids_host::wire::{ScheduleCreateInput, TaskAddInput};
use serde::Serialize;
use tauri::{AppHandle, Manager, State, WebviewWindow};
use tauri_plugin_opener::OpenerExt;

use crate::adapter::AdapterState;

fn message(error: impl std::fmt::Display) -> String {
    error.to_string()
}

/// Information about the running RAPIDS build, sourced from the host bundle.
#[derive(Serialize)]
pub struct AppInfo {
    pub name: String,
    pub version: String,
}

#[tauri::command]
pub fn app_version(app: AppHandle) -> AppInfo {
    let package = app.package_info();
    AppInfo {
        name: package.name.clone(),
        version: package.version.to_string(),
    }
}

#[tauri::command]
pub fn download_default_dir(app: AppHandle) -> Result<String, String> {
    app.path()
        .download_dir()
        .map(|path| path.to_string_lossy().into_owned())
        .map_err(message)
}

/// Opens a completed file through the plugin's native opener. The webview's
/// opener scope is intentionally not widened: the command only accepts an
/// existing file and delegates to the same operating-system association.
#[tauri::command]
pub fn open_download_file(app: AppHandle, path: String) -> Result<(), String> {
    if !std::path::Path::new(&path).is_file() {
        return Err("The downloaded file is no longer at its destination.".to_string());
    }
    app.opener()
        .open_path(path, None::<String>)
        .map_err(message)
}

#[tauri::command]
pub async fn tasks_list(state: State<'_, AdapterState>) -> Result<Vec<TaskView>, String> {
    state.host.tasks_list().await.map_err(message)
}

#[tauri::command]
pub async fn task_add(
    app: AppHandle,
    state: State<'_, AdapterState>,
    request: TaskAddInput,
) -> Result<TaskView, String> {
    let fallback_dir = if request.needs_default_dir() {
        let configured = state
            .host
            .settings_get()
            .await
            .map_err(message)?
            .default_dest_dir;
        Some(if configured.trim().is_empty() {
            app.path().download_dir().map_err(message)?
        } else {
            PathBuf::from(configured)
        })
    } else {
        None
    };
    let request = request.into_request(fallback_dir).map_err(message)?;
    state.host.task_add(request).await.map_err(message)
}

#[tauri::command]
pub async fn sniff_open(app: AppHandle, window: WebviewWindow, url: String) -> Result<(), String> {
    crate::sniff::open(&app, &window, &url)
}

#[tauri::command]
pub async fn task_pause(state: State<'_, AdapterState>, id: i64) -> Result<(), String> {
    state.host.task_pause(id).await.map_err(message)
}

#[tauri::command]
pub async fn task_resume(state: State<'_, AdapterState>, id: i64) -> Result<(), String> {
    state.host.task_resume(id).await.map_err(message)
}

#[tauri::command]
pub async fn task_retry(state: State<'_, AdapterState>, id: i64) -> Result<(), String> {
    state.host.task_retry(id).await.map_err(message)
}

#[tauri::command]
pub async fn task_cancel(state: State<'_, AdapterState>, id: i64) -> Result<(), String> {
    state.host.task_cancel(id).await.map_err(message)
}

#[tauri::command]
pub async fn task_delete(
    state: State<'_, AdapterState>,
    id: i64,
    remove_file: bool,
) -> Result<(), String> {
    state
        .host
        .task_delete(id, remove_file)
        .await
        .map_err(message)
}

#[tauri::command]
pub async fn task_reorder(
    state: State<'_, AdapterState>,
    id: i64,
    priority: i64,
) -> Result<(), String> {
    state.host.task_reorder(id, priority).await.map_err(message)
}

#[tauri::command]
pub async fn queue_pause(state: State<'_, AdapterState>) -> Result<(), String> {
    state.host.queue_pause().await.map_err(message)
}

#[tauri::command]
pub async fn queue_resume(state: State<'_, AdapterState>) -> Result<(), String> {
    state.host.queue_resume().await.map_err(message)
}

#[tauri::command]
pub async fn queue_paused(state: State<'_, AdapterState>) -> Result<bool, String> {
    state.host.queue_paused().await.map_err(message)
}

#[tauri::command]
pub async fn settings_get(state: State<'_, AdapterState>) -> Result<Settings, String> {
    state.host.settings_get().await.map_err(message)
}

#[tauri::command]
pub async fn settings_update(
    state: State<'_, AdapterState>,
    name: String,
    value: serde_json::Value,
) -> Result<Settings, String> {
    state
        .host
        .settings_update(&name, &value)
        .await
        .map_err(message)
}

#[tauri::command]
pub async fn schedules_list(state: State<'_, AdapterState>) -> Result<Vec<ScheduleView>, String> {
    state.host.schedules_list().await.map_err(message)
}

#[tauri::command]
pub async fn schedule_create(
    state: State<'_, AdapterState>,
    input: ScheduleCreateInput,
) -> Result<ScheduleView, String> {
    state
        .host
        .schedule_create(input.into_input().map_err(message)?)
        .await
        .map_err(message)
}

#[tauri::command]
pub async fn schedule_set_enabled(
    state: State<'_, AdapterState>,
    id: i64,
    enabled: bool,
) -> Result<(), String> {
    state
        .host
        .schedule_set_enabled(id, enabled)
        .await
        .map_err(message)
}

#[tauri::command]
pub async fn schedule_delete(state: State<'_, AdapterState>, id: i64) -> Result<(), String> {
    state.host.schedule_delete(id).await.map_err(message)
}

#[tauri::command]
pub async fn queue_counts(state: State<'_, AdapterState>) -> Result<QueueCounts, String> {
    state.host.queue_counts().await.map_err(message)
}
