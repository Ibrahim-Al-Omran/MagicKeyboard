use std::mem;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::UI::Input::{
    GetRawInputDeviceInfoW, GetRawInputDeviceList, RAWINPUTDEVICELIST, RIDI_DEVICEINFO,
    RIDI_DEVICENAME, RID_DEVICE_INFO, RIM_TYPEHID, RIM_TYPEKEYBOARD, RIM_TYPEMOUSE,
};

fn main() {
    unsafe {
        let mut count = 0u32;
        let _ = GetRawInputDeviceList(None, &mut count, mem::size_of::<RAWINPUTDEVICELIST>() as u32);
        let mut list = vec![RAWINPUTDEVICELIST::default(); count as usize];
        let n = GetRawInputDeviceList(
            Some(list.as_mut_ptr()),
            &mut count,
            mem::size_of::<RAWINPUTDEVICELIST>() as u32,
        );
        list.truncate(n as usize);
        for entry in list {
            let name = device_name(entry.hDevice).unwrap_or_default();
            let apple = name.to_ascii_lowercase().contains("004c")
                || name.to_ascii_lowercase().contains("05ac")
                || name.to_ascii_lowercase().contains("0267");
            if !apple {
                continue;
            }
            let kind = match entry.dwType {
                t if t == RIM_TYPEMOUSE => "MOUSE",
                t if t == RIM_TYPEKEYBOARD => "KEYBOARD",
                t if t == RIM_TYPEHID => "HID",
                _ => "OTHER",
            };
            let info = device_info(entry.hDevice);
            println!("{kind} {info}");
            println!("  {name}");
        }
    }
}

fn device_name(handle: HANDLE) -> Option<String> {
    unsafe {
        let mut len = 0u32;
        GetRawInputDeviceInfoW(Some(handle), RIDI_DEVICENAME, None, &mut len);
        if len == 0 {
            return None;
        }
        let mut buf = vec![0u16; len as usize];
        let written = GetRawInputDeviceInfoW(
            Some(handle),
            RIDI_DEVICENAME,
            Some(buf.as_mut_ptr().cast()),
            &mut len,
        );
        if written == u32::MAX {
            return None;
        }
        Some(String::from_utf16_lossy(
            &buf[..buf.iter().position(|&c| c == 0).unwrap_or(buf.len())],
        ))
    }
}

fn device_info(handle: HANDLE) -> String {
    unsafe {
        let mut size = mem::size_of::<RID_DEVICE_INFO>() as u32;
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
            return "info-failed".into();
        }
        if info.dwType == RIM_TYPEHID {
            let hid = info.Anonymous.hid;
            format!(
                "vid={:04X} pid={:04X} usage={:04X}:{:04X}",
                hid.dwVendorId, hid.dwProductId, hid.usUsagePage, hid.usUsage
            )
        } else if info.dwType == RIM_TYPEKEYBOARD {
            "keyboard-info".into()
        } else {
            format!("dwType={:?}", info.dwType)
        }
    }
}
