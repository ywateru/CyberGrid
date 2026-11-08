#[cfg(any(
    test,
    all(target_os = "android", debug_assertions, feature = "adb-test-control")
))]
mod adb_control;
mod platform;
mod protocol;
mod services;
mod state;

use state::{LabConfig, LabState, View};
use std::sync::{atomic::Ordering, Arc};
use tauri::Manager;

#[tauri::command]
async fn lab_view(state: tauri::State<'_, Arc<LabState>>) -> Result<View, String> {
    Ok(state.view().await)
}
#[tauri::command]
async fn begin_login(
    state: tauri::State<'_, Arc<LabState>>,
    config: LabConfig,
) -> Result<(), String> {
    state.inner().start_login(config, false).await
}
#[tauri::command]
async fn resume_login(
    state: tauri::State<'_, Arc<LabState>>,
    config: LabConfig,
) -> Result<(), String> {
    state.inner().start_login(config, true).await
}
#[tauri::command]
async fn cancel_login(state: tauri::State<'_, Arc<LabState>>) -> Result<(), String> {
    state.cancel_login().await
}
#[tauri::command]
async fn logout(state: tauri::State<'_, Arc<LabState>>) -> Result<(), String> {
    state.shutdown(true).await
}
#[tauri::command]
async fn refresh_devices(state: tauri::State<'_, Arc<LabState>>) -> Result<(), String> {
    state.refresh_devices().await
}
#[tauri::command]
async fn connect_device(
    state: tauri::State<'_, Arc<LabState>>,
    device_id: String,
) -> Result<(), String> {
    state.connect_device(device_id).await
}
#[tauri::command]
async fn disconnect_device(state: tauri::State<'_, Arc<LabState>>) -> Result<(), String> {
    state.disconnect_device().await
}
#[tauri::command]
async fn send_message(state: tauri::State<'_, Arc<LabState>>, text: String) -> Result<(), String> {
    state.send_message(text).await
}
#[tauri::command]
async fn probe_device(
    state: tauri::State<'_, Arc<LabState>>,
    device_id: String,
) -> Result<(), String> {
    state.probe_device(device_id).await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let state = LabState::new(app.path().app_data_dir()?, app.path().app_cache_dir()?)
                .map_err(std::io::Error::other)?;
            #[cfg(all(target_os = "android", debug_assertions, feature = "adb-test-control"))]
            adb_control::start(
                app.path().app_data_dir()?.join("files/adb-control"),
                state.clone(),
                app.handle().clone(),
            )?;
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            lab_view,
            begin_login,
            resume_login,
            cancel_login,
            logout,
            refresh_devices,
            connect_device,
            disconnect_device,
            send_message,
            probe_device
        ])
        .build(tauri::generate_context!())
        .expect("CyberGrid Lab could not start");
    app.run(|app, event| {
        let should_exit = matches!(
            event,
            tauri::RunEvent::ExitRequested { .. }
                | tauri::RunEvent::WindowEvent {
                    event: tauri::WindowEvent::CloseRequested { .. },
                    ..
                }
        );
        if should_exit {
            let state = app.state::<Arc<LabState>>().inner().clone();
            if !state.exiting.swap(true, Ordering::SeqCst) {
                match event {
                    tauri::RunEvent::ExitRequested { api, .. } => api.prevent_exit(),
                    tauri::RunEvent::WindowEvent {
                        event: tauri::WindowEvent::CloseRequested { api, .. },
                        ..
                    } => api.prevent_close(),
                    _ => {}
                }
                let handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    if state.shutdown(false).await.is_ok() {
                        handle.exit(0);
                    } else {
                        state.exiting.store(false, Ordering::SeqCst);
                    }
                });
            }
        }
    });
}
