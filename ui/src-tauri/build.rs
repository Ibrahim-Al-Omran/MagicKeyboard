const SETTINGS_COMMANDS: &[&str] = &[
    "get_snapshot",
    "refresh_devices",
    "select_device",
    "set_fn_lock",
    "set_launch_at_startup",
    "set_active_profile",
    "save_profile",
    "get_diagnostics",
    "test_volume_up",
    "test_delete",
];

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(SETTINGS_COMMANDS)),
    )
    .expect("failed to prepare the Tauri settings application");
}
