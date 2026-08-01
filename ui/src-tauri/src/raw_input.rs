//! Per-device keyboard capture via the Windows Raw Input API.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::thread;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{HANDLE, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::{
    GetRawInputData, GetRawInputDeviceInfoW, GetRawInputDeviceList, RegisterRawInputDevices,
    HRAWINPUT, RAWINPUT, RAWINPUTDEVICE, RAWINPUTDEVICELIST, RAWINPUTHEADER, RIDEV_INPUTSINK,
    RIDEV_DEVNOTIFY, RIDEV_PAGEONLY, RIDI_DEVICEINFO, RIDI_DEVICENAME, RID_DEVICE_INFO, RID_INPUT,
    RIM_TYPEHID, RIM_TYPEKEYBOARD,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, RegisterClassW,
    TranslateMessage, CW_USEDEFAULT, MSG, WINDOW_EX_STYLE, WINDOW_STYLE, WM_INPUT,
    WM_INPUT_DEVICE_CHANGE, WNDCLASSW,
};

use crate::remap::{
    note_hid_event, note_status, queue_action_for_source_key, set_fn_held, set_raw_input_stats,
};

const HWND_MESSAGE: HWND = HWND(-3isize as *mut core::ffi::c_void);
const RI_KEY_BREAK: u16 = 1;

static STARTED: AtomicBool = AtomicBool::new(false);
static MAGIC_DEVICES: OnceLock<Mutex<HashSet<isize>>> = OnceLock::new();

fn magic_devices() -> &'static Mutex<HashSet<isize>> {
    MAGIC_DEVICES.get_or_init(|| Mutex::new(HashSet::new()))
}

pub fn start() {
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    thread::Builder::new()
        .name("magic-keyboard-rawinput".into())
        .spawn(|| {
            if let Err(error) = run_raw_input_thread() {
                note_status(format!("Raw Input failed: {error}"), Some(error));
                set_raw_input_stats(false, 0);
            }
        })
        .expect("raw input thread");
}

fn run_raw_input_thread() -> Result<(), String> {
    unsafe {
        let module = GetModuleHandleW(PCWSTR::null()).map_err(|error| error.to_string())?;
        let class_name = windows::core::w!("MagicKeyboardRawInput");
        let class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: HINSTANCE(module.0),
            lpszClassName: class_name,
            ..Default::default()
        };
        if RegisterClassW(&class) == 0 {
            return Err("RegisterClassW failed".into());
        }

        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class_name,
            windows::core::w!("MagicKeyboardRawInput"),
            WINDOW_STYLE::default(),
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            Some(HWND_MESSAGE),
            None,
            Some(HINSTANCE(module.0)),
            None,
        )
        .map_err(|error| format!("CreateWindowExW failed: {error}"))?;

        // Keyboard + consumer + Apple vendor page (PAGEONLY catches nested Fn/Eject usages).
        let devices = [
            RAWINPUTDEVICE {
                usUsagePage: 0x01,
                usUsage: 0x06,
                dwFlags: RIDEV_INPUTSINK | RIDEV_DEVNOTIFY,
                hwndTarget: hwnd,
            },
            RAWINPUTDEVICE {
                usUsagePage: 0x0C,
                usUsage: 0x01,
                dwFlags: RIDEV_INPUTSINK | RIDEV_DEVNOTIFY,
                hwndTarget: hwnd,
            },
            // Consumer Eject nested in Apple keyboard reports.
            RAWINPUTDEVICE {
                usUsagePage: 0x0C,
                usUsage: 0xB8,
                dwFlags: RIDEV_INPUTSINK | RIDEV_DEVNOTIFY,
                hwndTarget: hwnd,
            },
            // Entire Apple vendor page — Fn is usage 0x03 inside the keyboard collection.
            RAWINPUTDEVICE {
                usUsagePage: 0xFF00,
                usUsage: 0,
                dwFlags: RIDEV_INPUTSINK | RIDEV_PAGEONLY | RIDEV_DEVNOTIFY,
                hwndTarget: hwnd,
            },
            RAWINPUTDEVICE {
                usUsagePage: 0xFF01,
                usUsage: 0,
                dwFlags: RIDEV_INPUTSINK | RIDEV_PAGEONLY | RIDEV_DEVNOTIFY,
                hwndTarget: hwnd,
            },
            RAWINPUTDEVICE {
                usUsagePage: 0xFF00,
                usUsage: 0x14,
                dwFlags: RIDEV_INPUTSINK | RIDEV_DEVNOTIFY,
                hwndTarget: hwnd,
            },
        ];
        RegisterRawInputDevices(&devices, std::mem::size_of::<RAWINPUTDEVICE>() as u32)
            .map_err(|error| format!("RegisterRawInputDevices failed: {error}"))?;

        refresh_magic_devices();
        note_status("Raw Input listener ready".into(), None);
        probe_eject_visibility();

        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).into() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    Ok(())
}

fn refresh_magic_devices() {
    unsafe {
        let mut count = 0u32;
        let _ = GetRawInputDeviceList(
            None,
            &mut count,
            std::mem::size_of::<RAWINPUTDEVICELIST>() as u32,
        );
        if count == 0 {
            set_raw_input_stats(true, 0);
            return;
        }
        let mut list = vec![RAWINPUTDEVICELIST::default(); count as usize];
        let result = GetRawInputDeviceList(
            Some(list.as_mut_ptr()),
            &mut count,
            std::mem::size_of::<RAWINPUTDEVICELIST>() as u32,
        );
        if result == u32::MAX {
            set_raw_input_stats(true, 0);
            return;
        }
        list.truncate(result as usize);

        let mut magic = HashSet::new();
        let mut samples = Vec::new();
        for entry in &list {
            let name = device_name(entry.hDevice).unwrap_or_else(|| "<unnamed>".into());
            let vid_pid = device_vid_pid(entry.hDevice)
                .map(|(v, p)| format!("VID_{v:04X}&PID_{p:04X}"))
                .unwrap_or_else(|| "no-vidpid".into());
            if samples.len() < 6 {
                samples.push(format!(
                    "{:?}:{} {}",
                    entry.dwType, vid_pid, truncate_path(&name, 64)
                ));
            }
            if is_magic_device(entry.hDevice) {
                magic.insert(entry.hDevice.0 as isize);
            }
        }
        let magic_count = magic.len();
        if let Ok(mut guard) = magic_devices().lock() {
            *guard = magic;
        }
        set_raw_input_stats(true, magic_count);
        note_status(
            format!(
                "Raw Input: {magic_count} Apple handle(s) of {} total. Samples: {}",
                list.len(),
                samples.join(" | ")
            ),
            None,
        );
    }
}

/// A1644 Eject is nested in the boot keyboard TLC (consumer usage 0xB8).
/// Windows opens that collection exclusively, so Raw Input only yields
/// RIM_TYPEKEYBOARD packets with the eject bit stripped. Older AHK scripts
/// work on Apple Wireless Keyboards that expose Fn/Eject as a separate
/// RIM_TYPEHID collection — this Bluetooth Magic Keyboard does not.
fn probe_eject_visibility() {
    let Ok(api) = hidapi::HidApi::new() else {
        return;
    };
    let mut keyboard_open: Option<String> = None;
    let mut vendor_open: Option<String> = None;
    for info in api.device_list() {
        let vid = info.vendor_id();
        if vid != 0x004c && vid != 0x05ac {
            continue;
        }
        let page = info.usage_page();
        let usage = info.usage();
        if page == 0x01 && usage == 0x06 && keyboard_open.is_none() {
            keyboard_open = Some(match info.open_device(&api) {
                Ok(_) => "opened (unexpected — eject HID path available)".into(),
                Err(error) => format!("Access denied / locked by Windows ({error})"),
            });
        } else if page == 0xff00 && usage == 0x14 && vendor_open.is_none() {
            vendor_open = Some(match info.open_device(&api) {
                Ok(_) => "opened OK (no Eject usage in this collection)".into(),
                Err(error) => format!("failed ({error})"),
            });
        }
    }
    note_status(
        format!(
            "Eject probe: keyboard HID {} | vendor HID {}. \
             Eject bit never reaches user-mode on A1644 BT; needs HID lower filter.",
            keyboard_open.as_deref().unwrap_or("not found"),
            vendor_open.as_deref().unwrap_or("not found"),
        ),
        None,
    );
}

fn truncate_path(path: &str, max: usize) -> String {
    if path.len() <= max {
        path.to_owned()
    } else {
        format!("…{}", &path[path.len() - max..])
    }
}

fn is_magic_device(handle: HANDLE) -> bool {
    if let Some((vid, pid)) = device_vid_pid(handle) {
        if (vid == 0x004c || vid == 0x05ac) && pid != 0 {
            return true;
        }
    }
    device_name(handle)
        .map(|name| {
            let lower = name.to_ascii_lowercase();
            lower.contains("vid_004c")
                || lower.contains("vid_05ac")
                || lower.contains("vid&0001004c")
                || lower.contains("vid&000005ac")
                || (lower.contains("004c") && lower.contains("0267"))
        })
        .unwrap_or(false)
}

fn device_vid_pid(handle: HANDLE) -> Option<(u16, u16)> {
    unsafe {
        let mut size = std::mem::size_of::<RID_DEVICE_INFO>() as u32;
        let mut info = RID_DEVICE_INFO {
            cbSize: size,
            ..Default::default()
        };
        let written = GetRawInputDeviceInfoW(
            Some(handle),
            RIDI_DEVICEINFO,
            Some((&raw mut info).cast()),
            &mut size,
        );
        if written == u32::MAX {
            return None;
        }
        if info.dwType == RIM_TYPEHID {
            let hid = info.Anonymous.hid;
            return Some((hid.dwVendorId as u16, hid.dwProductId as u16));
        }
        // Keyboard devices still often expose a HID path name with VID/PID.
        None
    }
}

fn device_name(handle: HANDLE) -> Option<String> {
    unsafe {
        let mut name_len = 0u32;
        GetRawInputDeviceInfoW(Some(handle), RIDI_DEVICENAME, None, &mut name_len);
        if name_len == 0 {
            return None;
        }
        let mut name_buf = vec![0u16; name_len as usize];
        let written = GetRawInputDeviceInfoW(
            Some(handle),
            RIDI_DEVICENAME,
            Some(name_buf.as_mut_ptr().cast()),
            &mut name_len,
        );
        if written == u32::MAX {
            return None;
        }
        Some(String::from_utf16_lossy(
            &name_buf[..name_buf.iter().position(|&c| c == 0).unwrap_or(name_buf.len())],
        ))
    }
}

fn is_tracked_magic(device: isize, handle: HANDLE) -> bool {
    let known = magic_devices()
        .lock()
        .map(|set| set.contains(&device))
        .unwrap_or(false);
    if known {
        return true;
    }
    if is_magic_device(handle) {
        if let Ok(mut set) = magic_devices().lock() {
            set.insert(device);
            set_raw_input_stats(true, set.len());
        }
        return true;
    }
    false
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_INPUT => {
            handle_wm_input(lparam);
            LRESULT(0)
        }
        WM_INPUT_DEVICE_CHANGE => {
            refresh_magic_devices();
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

fn handle_wm_input(lparam: LPARAM) {
    unsafe {
        let mut size = 0u32;
        let raw_handle = HRAWINPUT(lparam.0 as *mut _);
        GetRawInputData(
            raw_handle,
            RID_INPUT,
            None,
            &mut size,
            std::mem::size_of::<RAWINPUTHEADER>() as u32,
        );
        if size == 0 {
            return;
        }
        let mut buffer = vec![0u8; size as usize];
        let written = GetRawInputData(
            raw_handle,
            RID_INPUT,
            Some(buffer.as_mut_ptr().cast()),
            &mut size,
            std::mem::size_of::<RAWINPUTHEADER>() as u32,
        );
        if written == u32::MAX {
            return;
        }

        let raw = &*(buffer.as_ptr() as *const RAWINPUT);
        let device = raw.header.hDevice.0 as isize;
        let from_magic = is_tracked_magic(device, raw.header.hDevice);

        if raw.header.dwType == RIM_TYPEKEYBOARD.0 {
            let keyboard = raw.data.keyboard;
            if keyboard.Flags & RI_KEY_BREAK != 0 {
                return;
            }
            let vk = keyboard.VKey;
            let source = vk_to_source_key(vk);
            let tag = if from_magic { "MK" } else { "other" };
            note_hid_event(
                format!(
                    "Raw[{tag}] VK=0x{vk:02X} make=0x{:02X}{}",
                    keyboard.MakeCode,
                    source
                        .as_ref()
                        .map(|key| format!(" -> {key}"))
                        .unwrap_or_default()
                ),
                None,
            );
            // Remap F-row / media aliases from any keyboard while debugging input path.
            if let Some(source_key) = source {
                let _ = queue_action_for_source_key(&source_key);
            }
            return;
        }

        if raw.header.dwType == RIM_TYPEHID.0 {
            let hid = &raw.data.hid;
            let data_size = hid.dwSizeHid as usize;
            let count = hid.dwCount as usize;
            if data_size == 0 || count == 0 {
                return;
            }
            let data = std::slice::from_raw_parts(
                std::ptr::addr_of!(hid.bRawData) as *const u8,
                data_size * count,
            );
            // Always log HID payloads so we can see Apple vendor/consumer traffic.
            handle_hid_payload(data, from_magic);
        }
    }
}

fn handle_hid_payload(data: &[u8], from_magic: bool) {
    let tag = if from_magic { "MK" } else { "other" };
    // Report 0x01 keyboard payload from this Magic Keyboard.
    // Observed wire shape (10 bytes after report id):
    // [0] modifiers  [1] Apple FF00:0003 byte (Fn in bit0x10)
    // [2..7] keycodes  [8] consumer bits (bit0 = Eject 0xB8)
    let (report_id, payload) = match data.first().copied() {
        Some(0x01) => (Some(0x01u8), &data[1..]),
        Some(id) => (Some(id), &data[1..]),
        None => (None, data),
    };

    if let Some(0x01) = report_id {
        if payload.len() >= 2 {
            // Descriptor: Usage Page FF00, Usage 0x03, 8x1-bit. Fn is bit 4 (0x10) on Apple BT.
            let apple_byte = payload[1];
            let fn_held = (apple_byte & 0x10) != 0 || (apple_byte & 0x01) != 0;
            set_fn_held(fn_held);
            if apple_byte != 0 {
                note_hid_event(
                    format!("Raw[{tag}] AppleByte=0x{apple_byte:02X} fn={}", fn_held),
                    None,
                );
            }
        }
        if payload.len() >= 9 && (payload[8] & 0x01) != 0 {
            note_hid_event(format!("Raw[{tag}] Eject"), None);
            let _ = queue_action_for_source_key("Eject");
        }
    }

    // Standalone consumer / vendor reports.
    if data.iter().any(|&byte| byte == 0xB8) || consumer_eject_pressed(payload) {
        note_hid_event(format!("Raw[{tag}] Eject"), None);
        let _ = queue_action_for_source_key("Eject");
    }

    // Tiny vendor reports can be Fn-only (usage 0x03).
    if payload.len() <= 4 && payload.iter().any(|&b| b != 0) {
        let fn_held = payload.iter().any(|&b| (b & 0x11) != 0);
        if fn_held {
            set_fn_held(true);
            note_hid_event(format!("Raw[{tag}] Fn (short HID)"), None);
        } else if report_id != Some(0x01) {
            // Non-keyboard short zeroing reports release Fn.
            let all_clear = payload.iter().all(|&b| b == 0);
            if all_clear {
                set_fn_held(false);
            }
        }
        note_hid_event(
            format!(
                "Raw[{tag}] HID id={report_id:?} bytes={}",
                data.iter()
                    .take(12)
                    .map(|b| format!("{b:02X}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
            None,
        );
    }

    if payload.len() < 8 {
        return;
    }

    for &code in &payload[2..8.min(payload.len())] {
        if code == 0 {
            continue;
        }
        if let Some(source) = hid_code_to_source_key(code) {
            note_hid_event(format!("Raw[{tag}] HID 0x{code:02X} -> {source}"), None);
            let _ = queue_action_for_source_key(&source);
        } else {
            note_hid_event(format!("Raw[{tag}] HID keycode 0x{code:02X}"), None);
        }
    }
}

fn consumer_eject_pressed(payload: &[u8]) -> bool {
    // Array-style consumer reports: little-endian usage 0x00B8 in a 2-byte slot.
    payload.windows(2).any(|pair| pair == [0xB8, 0x00])
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
        _ => None,
    }
}

fn vk_to_source_key(vk: u16) -> Option<String> {
    match vk {
        0x70..=0x7B => Some(format!("F{}", vk - 0x6F)),
        0xB1 => Some("F7".into()),
        0xB3 => Some("F8".into()),
        0xB0 => Some("F9".into()),
        0xAD => Some("F10".into()),
        0xAE => Some("F11".into()),
        0xAF => Some("F12".into()),
        _ => None,
    }
}
