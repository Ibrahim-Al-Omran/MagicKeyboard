//! Launch Magic Keyboard when the user signs in to Windows.

use std::path::PathBuf;

const RUN_SUBKEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
/// Must match the NSIS installer hook and Tauri uninstall cleanup name.
pub const RUN_VALUE_NAME: &str = "Magic Keyboard";

pub fn is_enabled() -> bool {
    let Ok(key) = open_run_key(false) else {
        return false;
    };
    key.get_value::<String, _>(RUN_VALUE_NAME).is_ok()
}

pub fn set_enabled(enabled: bool) -> Result<(), String> {
    if enabled {
        let path = current_exe_quoted()?;
        let key = open_run_key(true)?;
        key.set_value(RUN_VALUE_NAME, &path)
            .map_err(|error| format!("Could not enable startup launch: {error}"))?;
    } else {
        let key = open_run_key(true)?;
        // Missing value is fine — already disabled.
        let _ = key.delete_value(RUN_VALUE_NAME);
    }
    Ok(())
}

pub fn sync(enabled: bool) -> Result<(), String> {
    if is_enabled() == enabled {
        return Ok(());
    }
    set_enabled(enabled)
}

fn current_exe_quoted() -> Result<String, String> {
    let path = std::env::current_exe().map_err(|error| error.to_string())?;
    let path = canonicalize_for_registry(path)?;
    Ok(format!("\"{}\"", path.display()))
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
