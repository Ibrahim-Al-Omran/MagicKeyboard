//! Read Magic Keyboard HID input reports directly.
//!
//! Apple top-row keys often do not appear as normal VK_F* events in a
//! low-level keyboard hook. Listening to the keyboard HID collection lets us
//! observe the real keycodes and fire remaps even when WH_KEYBOARD_LL is silent.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::Duration;

use hidapi::{HidApi, HidDevice};

use crate::remap::{note_hid_event, queue_action_for_source_key};

const APPLE_USB_VID: u16 = 0x05ac;
const APPLE_BT_VID: u16 = 0x004c;
const KEYBOARD_USAGE_PAGE: u16 = 0x01;
const KEYBOARD_USAGE: u16 = 0x06;

static LISTENER_STARTED: AtomicBool = AtomicBool::new(false);
static PRESSED: OnceLock<Mutex<HashSet<u8>>> = OnceLock::new();

fn pressed() -> &'static Mutex<HashSet<u8>> {
    PRESSED.get_or_init(|| Mutex::new(HashSet::new()))
}

pub fn start() {
    if LISTENER_STARTED.swap(true, Ordering::SeqCst) {
        return;
    }

    thread::Builder::new()
        .name("magic-keyboard-hid".into())
        .spawn(listen_loop)
        .expect("hid listener thread");
}

fn listen_loop() {
    loop {
        match open_keyboard() {
            Ok(device) => {
                note_hid_event("HID keyboard opened".into(), None);
                read_device(device);
                note_hid_event("HID keyboard closed; retrying".into(), None);
            }
            Err(error) => {
                note_hid_event(format!("HID open failed: {error}"), Some(error));
                thread::sleep(Duration::from_secs(2));
            }
        }
        thread::sleep(Duration::from_millis(500));
    }
}

fn open_keyboard() -> Result<HidDevice, String> {
    let api = HidApi::new().map_err(|error| error.to_string())?;
    for info in api.device_list() {
        let vid = info.vendor_id();
        if vid != APPLE_USB_VID && vid != APPLE_BT_VID {
            continue;
        }
        if info.usage_page() != KEYBOARD_USAGE_PAGE || info.usage() != KEYBOARD_USAGE {
            continue;
        }
        return info
            .open_device(&api)
            .map_err(|error| format!("open failed: {error}"));
    }
    Err("No Apple keyboard HID collection found".into())
}

fn read_device(device: HidDevice) {
    let mut buffer = [0u8; 65];
    loop {
        match device.read_timeout(&mut buffer, 500) {
            Ok(0) => continue,
            Ok(size) => handle_report(&buffer[..size]),
            Err(error) => {
                note_hid_event(format!("HID read error: {error}"), Some(error.to_string()));
                if let Ok(mut set) = pressed().lock() {
                    set.clear();
                }
                break;
            }
        }
    }
}

fn handle_report(report: &[u8]) {
    // Observed Magic Keyboard report 0x01 wire shape:
    // [0]=report_id 0x01
    // [1]=modifiers
    // [2]=vendor byte
    // [3..8]=six keycodes
    // [9...]=consumer/flags
    let (report_id, payload) = if report.first() == Some(&0x01) {
        (Some(0x01u8), &report[1..])
    } else {
        (None, report)
    };

    if payload.len() < 8 {
        note_hid_event(
            format!("HID short report id={report_id:?} len={}", report.len()),
            None,
        );
        return;
    }

    let keycodes = &payload[2..8];
    let mut current = HashSet::new();
    for &code in keycodes {
        if code != 0 {
            current.insert(code);
        }
    }

    // Also surface raw non-zero payload for debugging unknown top-row modes.
    if current.is_empty() {
        let interesting = payload.iter().any(|&byte| byte != 0);
        if interesting {
            note_hid_event(
                format!(
                    "HID report id={:?} bytes={}",
                    report_id,
                    report
                        .iter()
                        .map(|byte| format!("{byte:02X}"))
                        .collect::<Vec<_>>()
                        .join(" ")
                ),
                None,
            );
        }
    }

    let Ok(mut previous) = pressed().lock() else {
        return;
    };

    for code in current.difference(&previous).copied().collect::<Vec<_>>() {
        let source = hid_code_to_source_key(code);
        note_hid_event(
            format!(
                "HID press 0x{code:02X}{}",
                source
                    .as_ref()
                    .map(|key| format!(" -> {key}"))
                    .unwrap_or_default()
            ),
            None,
        );
        if let Some(source_key) = source {
            let _ = queue_action_for_source_key(&source_key);
        }
    }

    *previous = current;
}

fn hid_code_to_source_key(code: u8) -> Option<String> {
    match code {
        0x3a => Some("F1".into()),
        0x3b => Some("F2".into()),
        0x3c => Some("F3".into()),
        0x3d => Some("F4".into()),
        0x3e => Some("F5".into()),
        0x3f => Some("F6".into()),
        0x40 => Some("F7".into()),
        0x41 => Some("F8".into()),
        0x42 => Some("F9".into()),
        0x43 => Some("F10".into()),
        0x44 => Some("F11".into()),
        0x45 => Some("F12".into()),
        0x39 => Some("CapsLock".into()),
        0x29 => Some("Escape".into()),
        0x2c => Some("Space".into()),
        0x28 => Some("Enter".into()),
        0x2a => Some("Backspace".into()),
        0x2b => Some("Tab".into()),
        0x04..=0x1d => {
            let letter = (b'A' + (code - 0x04)) as char;
            Some(letter.to_string())
        }
        _ => None,
    }
}
