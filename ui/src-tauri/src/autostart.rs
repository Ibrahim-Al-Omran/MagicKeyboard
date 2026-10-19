//! Launch Magic Keyboard when the user signs in to Windows.

use std::path::PathBuf;

const RUN_SUBKEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
/// Must match the NSIS installer hook and Tauri uninstall cleanup name.
pub const RUN_VALUE_NAME: &str = "Magic Keyboard";
/// Passed by the sign-in launch so the app starts in the tray without a window.
/// Must match the NSIS installer hook.
pub const BACKGROUND_ARG: &str = "--background";

pub fn launched_in_background() -> bool {
    std::env::args().any(|arg| arg == BACKGROUND_ARG)
}

fn registered_command() -> Option<String> {
    open_run_key(false)
        .ok()?
        .get_value::<String, _>(RUN_VALUE_NAME)
        .ok()
}

pub fn set_enabled(enabled: bool) -> Result<(), String> {
    if enabled {
        let command = startup_command()?;
        let key = open_run_key(true)?;
        key.set_value(RUN_VALUE_NAME, &command)
            .map_err(|error| format!("Could not enable startup launch: {error}"))?;
    } else {
        let key = open_run_key(true)?;
        // Missing value is fine — already disabled.
        let _ = key.delete_value(RUN_VALUE_NAME);
    }
    Ok(())
}

pub fn sync(enabled: bool) -> Result<(), String> {
    let up_to_date = match registered_command() {
        Some(current) => enabled && Some(current) == startup_command().ok(),
        None => !enabled,
    };
    if up_to_date {
        return Ok(());
    }
    set_enabled(enabled)
}

fn startup_command() -> Result<String, String> {
    let path = std::env::current_exe().map_err(|error| error.to_string())?;
    let path = canonicalize_for_registry(path)?;
    Ok(format!("\"{}\" {BACKGROUND_ARG}", path.display()))
}

fn canonicalize_for_registry(path: PathBuf) -> Result<PathBuf, String> {
    let resolved = path.canonicalize().unwrap_or(path);
    // Windows canonicalize adds \\?\ prefix which breaks some Run key consumers.
    let text = resolved.to_string_lossy();
    let trimmed = text
        .strip_prefix(r"\\?\")
        .unwrap_or(text.as_ref())
        .to_owned();
    Ok(PathBuf::from(trimmed))
}

fn open_run_key(writable: bool) -> Result<winreg::RegKey, String> {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE};
    let hkcu = winreg::RegKey::predef(HKEY_CURRENT_USER);
    let access = if writable {
        KEY_READ | KEY_SET_VALUE
    } else {
        KEY_READ
    };
    hkcu.open_subkey_with_flags(RUN_SUBKEY, access)
        .map_err(|error| format!("Could not open Windows startup registry key: {error}"))
}
