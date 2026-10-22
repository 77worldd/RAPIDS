//! The desktop boundary: paths and clock come from the machine, commands go
//! to the pure host, and pushed events go to the main webview.

use std::sync::{Arc, Mutex};

use rapids_host::events::{EventSink, HostEvent};
use rapids_host::store::Store;
use rapids_host::wire::event_channel;
use rapids_host::{Clock, Host, HostError, ManagerHandle};
use tauri::{App, AppHandle, Emitter, Manager};
use tokio::task::JoinHandle;

pub struct AdapterState {
    pub host: Host,
    manager: ManagerHandle,
    task: Mutex<Option<JoinHandle<()>>>,
}

struct WebviewSink {
    app: AppHandle,
}

impl EventSink for WebviewSink {
    fn emit(&self, event: HostEvent) {
        // Listeners may not have mounted yet, or the window may be closing.
        // They take an authoritative snapshot through tasks_list on mount.
        let _ = self.app.emit_to("main", event_channel(&event), event);
    }
}

pub fn setup(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    // Windows resolves this to %LOCALAPPDATA%\RAPIDS\rapids.db. The store
    // creates the directory, migrates, and checks integrity before a window
    // can issue a command; a damaged database makes startup fail visibly.
    let database = app
        .path()
        .local_data_dir()?
        .join("RAPIDS")
        .join("rapids.db");
    let clock = Clock::at_offset_minutes(crate::platform::local_offset_minutes()?);
    let sink: Arc<dyn EventSink> = Arc::new(WebviewSink {
        app: app.handle().clone(),
    });
    let (store, manager, task) = tauri::async_runtime::block_on(async {
        let store = Arc::new(Store::open(&database).await?);
        let (manager, task) = rapids_host::manager::spawn(Arc::clone(&store), sink, clock);
        Ok::<_, HostError>((store, manager, task))
    })?;
    if !app.manage(AdapterState {
        host: Host::new(store, manager.clone()),
        manager,
        task: Mutex::new(Some(task)),
    }) {
        return Err(std::io::Error::other("the download host is already registered").into());
    }
    Ok(())
}

pub fn shutdown(app: &AppHandle) {
    if let Some(state) = app.try_state::<AdapterState>() {
        tauri::async_runtime::block_on(async {
            state.manager.shutdown().await;
            let task = {
                state
                    .task
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .take()
            };
            if let Some(task) = task {
                let _ = task.await;
            }
        });
    }
}
