//! User-mode keyboard remapper.
//!
//! Uses a low-level keyboard hook. Actions are executed on a worker thread
//! because SendInput/WMI from inside the hook callback is unreliable on Windows.

use std::collections::{HashMap, VecDeque};
use std::os::windows::process::CommandExt;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Mutex, OnceLock};
use std::thread;

use serde::Serialize;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Power::SetSuspendState;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    MapVirtualKeyW, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT,
    KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, MAPVK_VK_TO_VSC, VIRTUAL_KEY,
    VK_CONTROL, VK_F1, VK_F12, VK_H, VK_LWIN, VK_MEDIA_NEXT_TRACK, VK_MEDIA_PLAY_PAUSE,
    VK_MEDIA_PREV_TRACK, VK_MENU, VK_RCONTROL, VK_S, VK_SHIFT, VK_TAB, VK_VOLUME_DOWN,
    VK_VOLUME_MUTE, VK_VOLUME_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, SetWindowsHookExW, TranslateMessage,
    UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT, LLKHF_INJECTED, MSG, WH_KEYBOARD_LL, WM_KEYDOWN,
    WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

use crate::model::{KeyAction, KeyMapping, MediaCommand};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

static HOOK: Mutex<Option<isize>> = Mutex::new(None);
static LIVE: OnceLock<Mutex<LiveRemapState>> = OnceLock::new();
static ACTION_TX: OnceLock<Sender<KeyAction>> = OnceLock::new();
static SUPPRESSING: AtomicBool = AtomicBool::new(false);
static HOOK_INSTALLED: AtomicBool = AtomicBool::new(false);
static HOOK_THREAD_STARTED: AtomicBool = AtomicBool::new(false);
static KEYS_SEEN: AtomicU64 = AtomicU64::new(0);
/// Apple Fn key state from HID (not a normal Windows modifier).
static FN_HELD: AtomicBool = AtomicBool::new(false);
/// Windows cannot see Apple Fn without a driver. Right Ctrl acts as momentary Fn.
static SOFTWARE_FN_HELD: AtomicBool = AtomicBool::new(false);
static DIAGNOSTICS: OnceLock<Mutex<Diagnostics>> = OnceLock::new();

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostics {
    pub hook_installed: bool,
    pub remapping_enabled: bool,
    pub require_magic_keyboard: bool,
    pub magic_keyboard_present: bool,
    pub fn_lock: bool,
    pub fn_held: bool,
    pub binding_count: usize,
    pub raw_input_ready: bool,
    pub raw_magic_devices: usize,
    pub keys_seen: u64,
    pub last_vk: Option<u16>,
    pub last_key_name: Option<String>,
    pub last_action: Option<String>,
    pub last_error: Option<String>,
    pub last_status: Option<String>,
    pub recent_keys: Vec<String>,
}

impl Default for Diagnostics {
    fn default() -> Self {
        Self {
            hook_installed: false,
            remapping_enabled: false,
            require_magic_keyboard: false,
            magic_keyboard_present: false,
            fn_lock: false,
            fn_held: false,
            binding_count: 0,
            raw_input_ready: false,
            raw_magic_devices: 0,
            keys_seen: 0,
            last_vk: None,
            last_key_name: None,
            last_action: None,
            last_error: None,
            last_status: None,
            recent_keys: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
struct LiveRemapState {
    magic_keyboard_present: bool,
    /// When true, remaps only apply if a Magic Keyboard is connected.
    require_magic_keyboard: bool,
    remapping_enabled: bool,
    fn_lock: bool,
    /// Source keys that have a Windows virtual-key code (F1, CapsLock, …).
    bindings: HashMap<u16, KeyAction>,
    /// All source keys by normalized name (includes Eject and other HID-only keys).
    named_bindings: HashMap<String, KeyAction>,
}

impl Default for LiveRemapState {
    fn default() -> Self {
        Self {
            magic_keyboard_present: false,
            // Apply remaps while the app is open. Detection is still shown in the UI.
            require_magic_keyboard: false,
            remapping_enabled: true,
            fn_lock: false,
            bindings: HashMap::new(),
            named_bindings: HashMap::new(),
        }
    }
}

fn live() -> &'static Mutex<LiveRemapState> {
    LIVE.get_or_init(|| Mutex::new(LiveRemapState::default()))
}

fn diagnostics() -> &'static Mutex<Diagnostics> {
    DIAGNOSTICS.get_or_init(|| Mutex::new(Diagnostics::default()))
}

pub fn start() -> Result<(), String> {
    start_action_worker();
    start_hook_thread()
}

pub fn diagnostics_snapshot() -> Diagnostics {
    let mut snap = diagnostics()
        .lock()
        .map(|guard| guard.clone())
        .unwrap_or_default();
    if let Ok(state) = live().lock() {
        snap.remapping_enabled = state.remapping_enabled;
        snap.require_magic_keyboard = state.require_magic_keyboard;
        snap.magic_keyboard_present = state.magic_keyboard_present;
        snap.fn_lock = state.fn_lock;
        snap.binding_count = state.named_bindings.len().max(state.bindings.len());
    }
    snap.hook_installed = HOOK_INSTALLED.load(Ordering::SeqCst);
    snap.keys_seen = KEYS_SEEN.load(Ordering::SeqCst);
    snap.fn_held = fn_active();
    snap
}

pub fn update(
    magic_keyboard_present: bool,
    fn_lock: bool,
    mappings: &[KeyMapping],
) -> Result<(), String> {
    let mut bindings = HashMap::new();
    let mut named_bindings = HashMap::new();
    for mapping in mappings {
        let name = mapping.source_key.trim().to_ascii_lowercase();
        if name.is_empty() {
            continue;
        }
        named_bindings.insert(name, mapping.action.clone());
        if let Some(vk) = virtual_key_from_name(&mapping.source_key) {
            bindings.insert(vk, mapping.action.clone());
        }
    }

    let mut state = live()
        .lock()
        .map_err(|_| "Remapper state lock is poisoned.".to_owned())?;
    state.magic_keyboard_present = magic_keyboard_present;
    state.fn_lock = fn_lock;
    state.bindings = bindings;
    state.named_bindings = named_bindings;
    Ok(())
}

pub fn test_volume_up() -> Result<(), String> {
    queue_action(KeyAction::Media {
        command: MediaCommand::VolumeUp,
    })
}

pub fn test_delete() -> Result<(), String> {
    note_action("test:Delete".into(), None);
    SUPPRESSING.store(true, Ordering::SeqCst);
    let result = tap_delete();
    SUPPRESSING.store(false, Ordering::SeqCst);
    result
}

/// Update Apple Fn held state from HID reports.
pub fn set_fn_held(held: bool) {
    let previous = FN_HELD.swap(held, Ordering::SeqCst);
    if previous != held {
        note_status(
            if held {
                "Apple Fn held".into()
            } else {
                "Apple Fn released".into()
            },
            None,
        );
    }
}

fn set_software_fn_held(held: bool) {
    let previous = SOFTWARE_FN_HELD.swap(held, Ordering::SeqCst);
    if previous != held {
        note_status(
            if held {
                "Fn (Right Ctrl) held — F-row sends standard F-keys".into()
            } else {
                "Fn (Right Ctrl) released — F-row sends specials".into()
            },
            None,
        );
    }
}

pub fn fn_active() -> bool {
    FN_HELD.load(Ordering::SeqCst) || SOFTWARE_FN_HELD.load(Ordering::SeqCst)
}

pub fn prefer_function_specials(fn_lock: bool) -> bool {
    let fn_held = fn_active();
    // Apple default (fn_lock=false): specials unless Fn is held.
    // Function lock (fn_lock=true): standard F-keys unless Fn is held.
    if fn_lock {
        fn_held
    } else {
        !fn_held
    }
}

pub fn queue_action_for_source_key(source_key: &str) -> Result<(), String> {
    let Ok(state) = live().lock() else {
        return Err("Remapper state lock is poisoned.".into());
    };
    if !state.remapping_enabled {
        return Err("Remapping is disabled.".into());
    }
    let normalized = source_key.trim().to_ascii_lowercase();
    let is_f_row = matches!(
        normalized.as_str(),
        "f1" | "f2" | "f3" | "f4" | "f5" | "f6" | "f7" | "f8" | "f9" | "f10" | "f11" | "f12"
    );
    if is_f_row && !prefer_function_specials(state.fn_lock) {
        // Fn held (or function lock): let the real F-key through / don't remap.
        return Ok(());
    }
    let Some(action) = state.named_bindings.get(&normalized).cloned() else {
        return Err(format!("No binding for {source_key}"));
    };
    drop(state);
    note_action(format!("hid:{source_key}"), None);
    queue_action(action)
}

pub fn note_status(message: String, error: Option<String>) {
    if let Ok(mut diag) = diagnostics().lock() {
        diag.last_status = Some(message);
        if error.is_some() {
            diag.last_error = error;
        }
    }
}

pub fn note_hid_event(message: String, error: Option<String>) {
    // Key-path events only — do not overwrite with open/close spam.
    if let Ok(mut diag) = diagnostics().lock() {
        diag.last_key_name = Some(message.clone());
        let mut recent = VecDeque::from(diag.recent_keys.clone());
        recent.push_back(message);
        while recent.len() > 12 {
            recent.pop_front();
        }
        diag.recent_keys = recent.into_iter().collect();
        if error.is_some() {
            diag.last_error = error;
        }
    }
}

pub fn set_raw_input_stats(ready: bool, magic_devices: usize) {
    if let Ok(mut diag) = diagnostics().lock() {
        diag.raw_input_ready = ready;
        diag.raw_magic_devices = magic_devices;
    }
}

fn start_action_worker() {
    if ACTION_TX.get().is_some() {
        return;
    }
    let (tx, rx) = mpsc::channel::<KeyAction>();
    let _ = ACTION_TX.set(tx);
    thread::Builder::new()
        .name("magic-keyboard-actions".into())
        .spawn(move || {
            while let Ok(action) = rx.recv() {
                match execute_action(&action) {
                    Ok(()) => note_action(describe_action(&action), None),
                    Err(error) => note_action(describe_action(&action), Some(error)),
                }
            }
        })
        .expect("action worker");
}

fn start_hook_thread() -> Result<(), String> {
    if HOOK_THREAD_STARTED.swap(true, Ordering::SeqCst) {
        return Ok(());
    }

    let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();

    // WH_KEYBOARD_LL only delivers events to the installing thread's message
    // queue. Tauri's main loop is not a reliable pump for that, so own one.
    thread::Builder::new()
        .name("magic-keyboard-hook".into())
        .spawn(move || {
            let result = install_hook_on_current_thread();
            let _ = ready_tx.send(result.clone());
            if let Err(error) = result {
                note_status(format!("Keyboard hook failed: {error}"), Some(error));
                return;
            }
            note_status("Keyboard hook thread ready".into(), None);
            unsafe {
                let mut message = MSG::default();
                while GetMessageW(&mut message, None, 0, 0).into() {
                    let _ = TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
        })
        .map_err(|error| format!("Failed to start hook thread: {error}"))?;

    ready_rx
        .recv_timeout(std::time::Duration::from_secs(2))
        .map_err(|_| "Keyboard hook thread timed out.".to_owned())?
}

fn install_hook_on_current_thread() -> Result<(), String> {
    let mut guard = HOOK
        .lock()
        .map_err(|_| "Hook lock is poisoned.".to_owned())?;
    if guard.is_some() {
        return Ok(());
    }

    unsafe {
        let module = GetModuleHandleW(PCWSTR::null()).map_err(|error| error.to_string())?;
        let hook = SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(keyboard_proc),
            Some(HINSTANCE(module.0)),
            0,
        )
        .map_err(|error| format!("SetWindowsHookExW failed: {error}"))?;
        *guard = Some(hook.0 as isize);
    }
    HOOK_INSTALLED.store(true, Ordering::SeqCst);
    if let Ok(mut diag) = diagnostics().lock() {
        diag.hook_installed = true;
        diag.last_error = None;
    }
    Ok(())
}

#[allow(dead_code)]
fn uninstall_hook() -> Result<(), String> {
    let mut guard = HOOK
        .lock()
        .map_err(|_| "Hook lock is poisoned.".to_owned())?;
    if let Some(raw) = guard.take() {
        unsafe {
            UnhookWindowsHookEx(HHOOK(raw as *mut _)).map_err(|error| error.to_string())?;
        }
    }
    HOOK_INSTALLED.store(false, Ordering::SeqCst);
    Ok(())
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && !SUPPRESSING.load(Ordering::SeqCst) {
        let info = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        let injected = (info.flags.0 & LLKHF_INJECTED.0) != 0;
        if !injected {
            let message = wparam.0 as u32;
            let is_down = matches!(message, WM_KEYDOWN | WM_SYSKEYDOWN);
            let is_up = matches!(message, WM_KEYUP | WM_SYSKEYUP);
            let vk = info.vkCode as u16;
            // Right Ctrl = software Fn (Apple Fn is not visible to user-mode apps).
            if vk == VK_RCONTROL.0 {
                set_software_fn_held(is_down);
            }
            if is_down {
                note_key(vk);
            }
            if (is_down || is_up) && should_consume(vk, is_down) {
                return LRESULT(1);
            }
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

fn should_consume(vk: u16, is_down: bool) -> bool {
    let Ok(state) = live().lock() else {
        return false;
    };
    if !state.remapping_enabled {
        return false;
    }
    if state.require_magic_keyboard && !state.magic_keyboard_present {
        return false;
    }

    // Some Windows/Apple stacks deliver the F-row as media/consumer virtual keys
    // instead of VK_F7..VK_F12. Map those back to the F-row bindings.
    let lookup_vk = consumer_vk_to_function_vk(vk).unwrap_or(vk);
    let is_function_row = (VK_F1.0..=VK_F12.0).contains(&lookup_vk);
    let is_consumer_alias = consumer_vk_to_function_vk(vk).is_some();

    let Some(action) = state.bindings.get(&lookup_vk).cloned() else {
        return false;
    };
    let fn_lock = state.fn_lock;
    drop(state);

    if is_function_row || is_consumer_alias {
        if prefer_function_specials(fn_lock) {
            if is_down {
                let _ = queue_action(action);
            }
            return true;
        }
        // Want a bare standard F-key.
        // If Apple Fn were visible, Windows would already send F-keys alone.
        // With Right Ctrl as Fn, strip the modifier by consuming and re-injecting.
        if is_consumer_alias || SOFTWARE_FN_HELD.load(Ordering::SeqCst) {
            if is_down {
                let f_index = lookup_vk - VK_F1.0 + 1;
                let _ = queue_action(KeyAction::Key {
                    key: format!("F{f_index}"),
                });
            }
            return true;
        }
        return false;
    }

    // Non-F-row remaps (Eject → Delete, CapsLock, …) always apply.
    if is_down {
        let _ = queue_action(action);
    }
    true
}

fn consumer_vk_to_function_vk(vk: u16) -> Option<u16> {
    match vk {
        0xB1 => Some(0x76), // previous -> F7
        0xB3 => Some(0x77), // play/pause -> F8
        0xB0 => Some(0x78), // next -> F9
        0xAD => Some(0x79), // mute -> F10
        0xAE => Some(0x7A), // vol down -> F11
        0xAF => Some(0x7B), // vol up -> F12
        _ => None,
    }
}

fn queue_action(action: KeyAction) -> Result<(), String> {
    ACTION_TX
        .get()
        .ok_or_else(|| "Action worker is not running.".to_owned())?
        .send(action)
        .map_err(|_| "Failed to queue keyboard action.".to_owned())
}

fn note_key(vk: u16) {
    KEYS_SEEN.fetch_add(1, Ordering::SeqCst);
    let name = vk_name(vk);
    if let Ok(mut diag) = diagnostics().lock() {
        diag.last_vk = Some(vk);
        diag.last_key_name = Some(name.clone());
        diag.keys_seen = KEYS_SEEN.load(Ordering::SeqCst);
        let mut recent = VecDeque::from(diag.recent_keys.clone());
        recent.push_back(format!("LL {name} (0x{vk:02X})"));
        while recent.len() > 12 {
            recent.pop_front();
        }
        diag.recent_keys = recent.into_iter().collect();
    }
}

fn note_action(action: String, error: Option<String>) {
    if let Ok(mut diag) = diagnostics().lock() {
        diag.last_action = Some(action);
        diag.last_error = error;
    }
}

fn describe_action(action: &KeyAction) -> String {
    match action {
        KeyAction::Disabled => "disabled".into(),
        KeyAction::Key { key } => format!("key:{key}"),
        KeyAction::Shortcut { keys } => format!("shortcut:{}", keys.join("+")),
        KeyAction::Media { command } => format!("media:{command:?}"),
        KeyAction::Launch { target } => format!("launch:{target}"),
    }
}

fn execute_action(action: &KeyAction) -> Result<(), String> {
    SUPPRESSING.store(true, Ordering::SeqCst);
    let result = match action {
        KeyAction::Disabled => Ok(()),
        KeyAction::Key { key } => send_named_key(key),
        KeyAction::Shortcut { keys } => send_shortcut(keys),
        KeyAction::Media { command } => send_media(command),
        KeyAction::Launch { target } => launch_target(target),
    };
    SUPPRESSING.store(false, Ordering::SeqCst);
    result
}

fn send_media(command: &MediaCommand) -> Result<(), String> {
    let vk = match command {
        MediaCommand::PlayPause => VK_MEDIA_PLAY_PAUSE,
        MediaCommand::Previous => VK_MEDIA_PREV_TRACK,
        MediaCommand::Next => VK_MEDIA_NEXT_TRACK,
        MediaCommand::VolumeUp => VK_VOLUME_UP,
        MediaCommand::VolumeDown => VK_VOLUME_DOWN,
        MediaCommand::Mute => VK_VOLUME_MUTE,
    };
    tap_vk(vk)
}

fn send_named_key(name: &str) -> Result<(), String> {
    match name {
        "BrightnessDown" => adjust_brightness(-10),
        "BrightnessUp" => adjust_brightness(10),
        "Sleep" => {
            unsafe {
                let _ = SetSuspendState(false, false, false);
            }
            Ok(())
        }
        "Delete" | "Del" => tap_delete(),
        other => {
            if let Some(vk) = virtual_key_from_name(other) {
                tap_vk(VIRTUAL_KEY(vk))
            } else {
                Err(format!("Unsupported key action: {other}"))
            }
        }
    }
}

fn send_shortcut(keys: &[String]) -> Result<(), String> {
    let mut modifiers = Vec::new();
    let mut main = None;
    for key in keys {
        match key.to_ascii_lowercase().as_str() {
            "meta" | "win" | "super" | "cmd" | "command" => modifiers.push(VK_LWIN),
            "control" | "ctrl" => modifiers.push(VK_CONTROL),
            "alt" | "option" => modifiers.push(VK_MENU),
            "shift" => modifiers.push(VK_SHIFT),
            other => {
                let vk = virtual_key_from_name(other)
                    .ok_or_else(|| format!("Unsupported shortcut key: {other}"))?;
                main = Some(VIRTUAL_KEY(vk));
            }
        }
    }
    let main = main.ok_or_else(|| "Shortcut is missing a non-modifier key.".to_owned())?;

    for modifier in &modifiers {
        key_event(*modifier, false)?;
    }
    key_event(main, false)?;
    key_event(main, true)?;
    for modifier in modifiers.iter().rev() {
        key_event(*modifier, true)?;
    }
    Ok(())
}

fn launch_target(target: &str) -> Result<(), String> {
    let trimmed = target.trim();
    if trimmed.is_empty() {
        return Err("Launch target is empty.".to_owned());
    }
    let command = if trimmed.eq_ignore_ascii_case("calculator") {
        "calc.exe".to_owned()
    } else if trimmed.eq_ignore_ascii_case("notepad") {
        "notepad.exe".to_owned()
    } else if trimmed.eq_ignore_ascii_case("explorer") {
        "explorer.exe".to_owned()
    } else {
        trimmed.to_owned()
    };
    std::process::Command::new("cmd")
        .args(["/C", "start", "", &command])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|error| format!("Failed to launch {command}: {error}"))?;
    Ok(())
}

fn adjust_brightness(delta: i32) -> Result<(), String> {
    let hotkey = if delta < 0 {
        VIRTUAL_KEY(0x7D)
    } else {
        VIRTUAL_KEY(0x7C)
    };
    let _ = tap_vk(hotkey);

    let script = format!(
        "$m = Get-CimInstance -Namespace root/WMI -ClassName WmiMonitorBrightness -ErrorAction SilentlyContinue; \
         $methods = Get-CimInstance -Namespace root/WMI -ClassName WmiMonitorBrightnessMethods -ErrorAction SilentlyContinue; \
         if ($m -and $methods) {{ $next = [Math]::Max(0, [Math]::Min(100, [int]$m.CurrentBrightness + ({delta}))); \
         Invoke-CimMethod -InputObject $methods -MethodName WmiSetBrightness -Arguments @{{Timeout=1; Brightness=$next}} | Out-Null }}"
    );
    let status = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map_err(|error| format!("Brightness command failed: {error}"))?;
    if !status.success() {
        return Err("Brightness adjustment failed on this display.".into());
    }
    Ok(())
}

fn tap_vk(vk: VIRTUAL_KEY) -> Result<(), String> {
    if vk.0 == 0x2E {
        return tap_delete();
    }
    key_event(vk, false)?;
    key_event(vk, true)
}

/// Forward-Delete (not Backspace). Needs extended scan code 0xE0 0x53.
fn tap_delete() -> Result<(), String> {
    let down = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(0),
                wScan: 0x53,
                dwFlags: KEYEVENTF_SCANCODE | KEYEVENTF_EXTENDEDKEY,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let up = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(0),
                wScan: 0x53,
                dwFlags: KEYEVENTF_SCANCODE | KEYEVENTF_EXTENDEDKEY | KEYEVENTF_KEYUP,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    unsafe {
        let sent = SendInput(&[down, up], std::mem::size_of::<INPUT>() as i32);
        if sent != 2 {
            return Err(format!("SendInput Delete failed (sent {sent}/2)."));
        }
    }
    note_action("key:Delete".into(), None);
    Ok(())
}

fn is_extended_vk(vk: VIRTUAL_KEY) -> bool {
    matches!(
        vk.0,
        0x21 | 0x22 | 0x23 | 0x24 | // PageUp/PageDown/End/Home
        0x25 | 0x26 | 0x27 | 0x28 | // arrows
        0x2D | 0x2E | // Insert/Delete
        0x5B | 0x5C | // Left/Right Win
        0xA3 | 0xA5 | // Right Ctrl/Alt
        0x6F | // Numpad divide
        0xAD | 0xAE | 0xAF | // volume
        0xB0 | 0xB1 | 0xB3 // media
    )
}

fn key_event(vk: VIRTUAL_KEY, up: bool) -> Result<(), String> {
    let scan = (unsafe { MapVirtualKeyW(u32::from(vk.0), MAPVK_VK_TO_VSC) }) as u16;
    let mut flags = KEYBD_EVENT_FLAGS(0);
    if is_extended_vk(vk) {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    if up {
        flags |= KEYEVENTF_KEYUP;
    }

    let input = if scan != 0 {
        flags |= KEYEVENTF_SCANCODE;
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(0),
                    wScan: scan,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    } else {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    };
    unsafe {
        let sent = SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
        if sent != 1 {
            return Err(format!("SendInput failed for VK=0x{:02X}.", vk.0));
        }
    }
    Ok(())
}

fn vk_name(vk: u16) -> String {
    match vk {
        0x70..=0x7B => format!("F{}", vk - 0x6F),
        0xAD => "VolumeMute".into(),
        0xAE => "VolumeDown".into(),
        0xAF => "VolumeUp".into(),
        0xB0 => "NextTrack".into(),
        0xB1 => "PrevTrack".into(),
        0xB3 => "PlayPause".into(),
        _ => format!("VK_0x{vk:02X}"),
    }
}

fn virtual_key_from_name(name: &str) -> Option<u16> {
    let normalized = name.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "f1" => Some(VK_F1.0),
        "f2" => Some(0x71),
        "f3" => Some(0x72),
        "f4" => Some(0x73),
        "f5" => Some(0x74),
        "f6" => Some(0x75),
        "f7" => Some(0x76),
        "f8" => Some(0x77),
        "f9" => Some(0x78),
        "f10" => Some(0x79),
        "f11" => Some(0x7A),
        "f12" => Some(VK_F12.0),
        "escape" | "esc" => Some(0x1B),
        "tab" => Some(VK_TAB.0),
        "capslock" => Some(0x14),
        "space" => Some(0x20),
        "enter" | "return" => Some(0x0D),
        "backspace" => Some(0x08),
        "delete" | "del" => Some(0x2E),
        "insert" | "ins" => Some(0x2D),
        "home" => Some(0x24),
        "end" => Some(0x23),
        "pageup" | "page_up" | "pgup" => Some(0x21),
        "pagedown" | "page_down" | "pgdn" => Some(0x22),
        "left" | "arrowleft" => Some(0x25),
        "up" | "arrowup" => Some(0x26),
        "right" | "arrowright" => Some(0x27),
        "down" | "arrowdown" => Some(0x28),
        "printscreen" | "prtsc" => Some(0x2C),
        // Eject is HID/consumer-only on Magic Keyboard — no Windows VK.
        "eject" => None,
        "s" => Some(VK_S.0),
        "h" => Some(VK_H.0),
        other if other.len() == 1 => {
            let ch = other.chars().next()?.to_ascii_uppercase();
            if ch.is_ascii_alphanumeric() {
                Some(ch as u16)
            } else {
                None
            }
        }
        _ => None,
    }
}
