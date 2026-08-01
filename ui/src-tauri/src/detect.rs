//! Enumerate connected Apple Magic Keyboards through Windows HID APIs.

use crate::model::KeyboardModelId;
use hidapi::HidApi;

const APPLE_USB_VID: u16 = 0x05ac;
const APPLE_BT_VID: u16 = 0x004c;
const KEYBOARD_USAGE_PAGE: u16 = 0x01;
const KEYBOARD_USAGE: u16 = 0x06;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedKeyboard {
    pub id: String,
    pub name: String,
    pub model_id: KeyboardModelId,
    pub vendor_id: u16,
    pub product_id: u16,
    pub bus: String,
}

pub fn list_magic_keyboards() -> Result<Vec<DetectedKeyboard>, String> {
    let api = HidApi::new().map_err(|error| format!("HID discovery failed: {error}"))?;
    let mut devices = Vec::new();

    for (index, info) in api.device_list().enumerate() {
        let vendor_id = info.vendor_id();
        if vendor_id != APPLE_USB_VID && vendor_id != APPLE_BT_VID {
            continue;
        }
        if info.usage_page() != KEYBOARD_USAGE_PAGE || info.usage() != KEYBOARD_USAGE {
            continue;
        }

        let product_id = info.product_id();
        let bus = bus_label(info.bus_type());
        let product = info
            .product_string()
            .map(str::to_owned)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "Magic Keyboard".to_owned());
        let model_id = infer_model(product_id, &product);
        let display_name = if product_id == 0x0267 || product.to_ascii_lowercase().contains("a1644")
        {
            format!("Magic Keyboard A1644 ({bus})")
        } else {
            format!("{product} ({bus})")
        };
        let id = format!(
            "apple-{:04x}-{:04x}-{bus}-{index}",
            vendor_id, product_id
        );

        devices.push(DetectedKeyboard {
            id,
            name: display_name,
            model_id,
            vendor_id,
            product_id,
            bus: bus.to_owned(),
        });
    }

    devices.sort_by(|left, right| left.id.cmp(&right.id));
    devices.dedup_by(|left, right| {
        left.vendor_id == right.vendor_id
            && left.product_id == right.product_id
            && left.bus == right.bus
    });
    Ok(devices)
}

fn bus_label(bus: hidapi::BusType) -> &'static str {
    match bus {
        hidapi::BusType::Usb => "usb",
        hidapi::BusType::Bluetooth => "bluetooth",
        hidapi::BusType::I2c => "i2c",
        hidapi::BusType::Spi => "spi",
        _ => "unknown",
    }
}

fn infer_model(product_id: u16, product: &str) -> KeyboardModelId {
    let lower = product.to_ascii_lowercase();
    if lower.contains("numeric") || lower.contains("keypad") || product_id == 0x029f {
        return KeyboardModelId::MagicKeyboardNumeric;
    }
    // Touch ID models only — do not treat shared PIDs like 0x0267 as Touch ID.
    // 0x0267 is Magic Keyboard A1644 (compact, with Eject) over Bluetooth.
    if lower.contains("touch id") || product_id == 0x029c {
        return KeyboardModelId::MagicKeyboardTouchId;
    }
    KeyboardModelId::MagicKeyboardCompact
}
