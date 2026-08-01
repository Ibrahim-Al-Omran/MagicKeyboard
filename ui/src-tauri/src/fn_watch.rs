//! Watch Apple vendor HID for Fn-related reports.
//!
//! The real Apple Fn bit lives inside the boot keyboard collection, which
//! Windows opens exclusively. This watcher still attaches to the vendor
//! collection (ff00:0014) for diagnostics and any Fn-related traffic that
//! surfaces there. Momentary F-key access uses Right Ctrl in the remapper.

use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use hidapi::{HidApi, HidDevice};

use crate::remap::{note_status, set_fn_held};

const APPLE_USB_VID: u16 = 0x05ac;
const APPLE_BT_VID: u16 = 0x004c;
const VENDOR_USAGE_PAGE: u16 = 0xff00;
const VENDOR_USAGE: u16 = 0x14;

static STARTED: AtomicBool = AtomicBool::new(false);

pub fn start() {
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    thread::Builder::new()
        .name("magic-keyboard-fn".into())
        .spawn(listen_loop)
        .expect("fn watch thread");
}

fn listen_loop() {
    let mut warned = false;
    loop {
        match open_vendor() {
            Ok(device) => {
                note_status(
                    "Vendor HID open. Apple Fn may still need Right Ctrl (Windows blocks keyboard HID).".into(),
                    None,
                );
                warned = false;
                read_device(device);
                note_status("Fn watcher disconnected; retrying".into(), None);
            }
            Err(error) => {
                if !warned {
                    note_status(
                        format!(
                            "Apple Fn HID unavailable ({error}). Hold Right Ctrl for F-keys."
                        ),
                        None,
                    );
                    warned = true;
                }
                thread::sleep(Duration::from_secs(2));
            }
        }
        thread::sleep(Duration::from_millis(400));
    }
}

fn open_vendor() -> Result<HidDevice, String> {
    let api = HidApi::new().map_err(|error| error.to_string())?;
    for info in api.device_list() {
        let vid = info.vendor_id();
        if vid != APPLE_USB_VID && vid != APPLE_BT_VID {
            continue;
        }
        if info.usage_page() != VENDOR_USAGE_PAGE || info.usage() != VENDOR_USAGE {
            continue;
        }
        return info
            .open_device(&api)
            .map_err(|error| format!("vendor open failed: {error}"));
    }
    Err("No Apple vendor HID collection found".into())
}

fn read_device(device: HidDevice) {
    let mut buffer = [0u8; 64];
    loop {
        match device.read_timeout(&mut buffer, 500) {
            Ok(0) => continue,
            Ok(size) => update_fn_from_report(&buffer[..size]),
            Err(_) => {
                set_fn_held(false);
                break;
            }
        }
    }
}

fn update_fn_from_report(report: &[u8]) {
    if report.is_empty() {
        return;
    }
    let data = if report[0] >= 0x80 && report.len() > 1 {
        &report[1..]
    } else if report[0] <= 0x0F && report.len() > 1 {
        &report[1..]
    } else {
        report
    };
    let fn_held = data.iter().take(3).any(|byte| (byte & 0x11) != 0);
    set_fn_held(fn_held);
}
