# Magic Keyboard filter — third-party attributions

## WinAppleKey

The HIDCLASS lower-filter architecture, A1644 Bluetooth/USB buffer interception
(`IOCTL_INTERNAL_BTH_SUBMIT_BRB` / `IOCTL_INTERNAL_USB_SUBMIT_URB`), and A1644
payload rewrite logic in `driver/` are adapted from:

- Project: [WinAppleKey](https://github.com/samartzidis/WinAppleKey)
- Author: George Samartzidis
- License: MIT (see `WinAppleKey/LICENSE`)

WinAppleKey source is retained under `third_party/WinAppleKey/` for reference.
Our runtime driver code lives under `driver/` and is not a verbatim copy of
their Visual Studio project.
