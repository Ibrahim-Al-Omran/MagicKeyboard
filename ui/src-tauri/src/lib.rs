mod autostart;
mod detect;
mod fn_watch;
mod model;
mod raw_input;
mod remap;
mod state;

use std::sync::{Mutex, MutexGuard};

use model::{AppSnapshot, DeviceStatus, Profile};
use remap::Diagnostics;
use state::SettingsState;
use tauri::{WebviewUrl, WebviewWindowBuilder};

type SharedState = Mutex<SettingsState>;
type CommandResult<T> = Result<T, String>;

fn lock_state<'a>(
    state: &'a tauri::State<'_, SharedState>,
) -> CommandResult<MutexGuard<'a, SettingsState>> {
    state
        .lock()
        .map_err(|_| "Settings state is unavailable.".to_owned())
}

#[tauri::command]
fn get_snapshot(state: tauri::State<'_, SharedState>) -> CommandResult<AppSnapshot> {
    Ok(lock_state(&state)?.snapshot())
}

#[tauri::command]
fn refresh_devices(state: tauri::State<'_, SharedState>) -> CommandResult<AppSnapshot> {
    Ok(lock_state(&state)?.refresh_devices())
}

#[tauri::command]
fn select_device(
    state: tauri::State<'_, SharedState>,
    device_id: String,
) -> CommandResult<AppSnapshot> {
    lock_state(&state)?
        .select_device(&device_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn set_fn_lock(
    state: tauri::State<'_, SharedState>,
    device_id: String,
    enabled: bool,
) -> CommandResult<DeviceStatus> {
    lock_state(&state)?
        .set_fn_lock(&device_id, enabled)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn set_launch_at_startup(
    state: tauri::State<'_, SharedState>,
    enabled: bool,
) -> CommandResult<AppSnapshot> {
    lock_state(&state)?
        .set_launch_at_startup(enabled)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn set_active_profile(
    state: tauri::State<'_, SharedState>,
    device_id: String,
    profile_id: String,
) -> CommandResult<DeviceStatus> {
    lock_state(&state)?
        .set_active_profile(&device_id, &profile_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn save_profile(
    state: tauri::State<'_, SharedState>,
    profile: Profile,
) -> CommandResult<AppSnapshot> {
    lock_state(&state)?
        .save_profile(profile)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn get_diagnostics() -> Diagnostics {
    remap::diagnostics_snapshot()
}

#[tauri::command]
fn test_volume_up() -> CommandResult<()> {
    remap::test_volume_up()
}

#[tauri::command]
fn test_delete() -> CommandResult<()> {
    remap::test_delete()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    if let Err(error) = remap::start() {
        eprintln!("failed to start remapper: {error}");
    }
    raw_input::start();
    fn_watch::start();

    tauri::Builder::default()
        .manage(SharedState::default())
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            refresh_devices,
            select_device,
            set_fn_lock,
            set_launch_at_startup,
            set_active_profile,
            save_profile,
            get_diagnostics,
            test_volume_up,
            test_delete
        ])
        .setup(|app| {
            WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                .title("Magic Keyboard")
                .inner_size(980.0, 820.0)
                .min_inner_size(760.0, 600.0)
                .center()
                .devtools(cfg!(debug_assertions))
                .on_navigation(|url| {
                    url.scheme() == "tauri"
                        || (url.scheme() == "http" && url.host_str() == Some("tauri.localhost"))
                })
                .on_new_window(|_, _| tauri::webview::NewWindowResponse::Deny)
                .build()?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("failed to run Magic Keyboard");
}
