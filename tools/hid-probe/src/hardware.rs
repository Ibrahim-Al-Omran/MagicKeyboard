use crate::trace::DeviceMetadata;
#[cfg(not(windows))]
use anyhow::bail;
#[cfg(not(windows))]
use anyhow::Result;
use serde::Serialize;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, Copy, Default)]
pub struct DeviceFilter {
    pub vendor_id: Option<u16>,
    pub product_id: Option<u16>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceDescription {
    pub device: DeviceMetadata,
    pub report_descriptor: crate::descriptor::DescriptorData,
}

#[derive(Debug, Clone)]
pub struct CaptureOptions {
    pub filter: DeviceFilter,
    pub index: usize,
    pub output: PathBuf,
    pub duration: Option<Duration>,
    pub max_reports: Option<u64>,
    pub read_timeout: Duration,
    pub include_identifiers: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CaptureOutcome {
    pub output: PathBuf,
    pub report_count: u64,
    pub elapsed_us: u64,
    pub reason: String,
}

#[cfg(windows)]
mod platform {
    use super::{CaptureOptions, CaptureOutcome, DeviceDescription, DeviceFilter, DeviceMetadata};
    use crate::descriptor::DescriptorData;
    use crate::trace::{JsonlTraceWriter, TraceHeader};
    use crate::TOOL_VERSION;
    use anyhow::{bail, Context, Result};
    use hidapi::{DeviceInfo, HidApi, HidDevice, MAX_REPORT_DESCRIPTOR_SIZE};
    use std::fs::OpenOptions;
    use std::io::BufWriter;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    const MAX_INPUT_REPORT_SIZE: usize = u16::MAX as usize + 1;

    pub fn list_devices(
        filter: DeviceFilter,
        include_identifiers: bool,
    ) -> Result<Vec<DeviceMetadata>> {
        let api = HidApi::new().context("initialize Windows HID device discovery")?;
        Ok(api
            .device_list()
            .filter(|device| matches_filter(device, filter))
            .enumerate()
            .map(|(index, device)| metadata(device, index, include_identifiers))
            .collect())
    }

    pub fn describe_device(
        filter: DeviceFilter,
        index: usize,
        include_identifiers: bool,
    ) -> Result<DeviceDescription> {
        let api = HidApi::new().context("initialize Windows HID device discovery")?;
        let info = select_device(&api, filter, index)?;
        let device = info
            .open_device(&api)
            .with_context(|| format!("open matched HID device at index {index} read-only"))?;
        let report_descriptor = read_descriptor(&device)?;

        Ok(DeviceDescription {
            device: metadata(info, index, include_identifiers),
            report_descriptor,
        })
    }

    pub fn capture_device(options: CaptureOptions) -> Result<CaptureOutcome> {
        let timeout_ms = i32::try_from(options.read_timeout.as_millis())
            .context("read timeout exceeds the Windows HID API limit")?;
        if timeout_ms <= 0 {
            bail!("read timeout must be at least 1 ms");
        }

        let api = HidApi::new().context("initialize Windows HID device discovery")?;
        let info = select_device(&api, options.filter, options.index)?;
        let metadata = metadata(info, options.index, options.include_identifiers);
        let device = info.open_device(&api).with_context(|| {
            format!(
                "open matched HID device at index {} read-only",
                options.index
            )
        })?;

        let (report_descriptor, descriptor_error) = match read_descriptor(&device) {
            Ok(descriptor) => (Some(descriptor), None),
            Err(error) => {
                let message = if options.include_identifiers {
                    format!("{error:#}")
                } else {
                    "report descriptor unavailable through the Windows HID API".to_owned()
                };
                (None, Some(message))
            }
        };
        let created_unix_us = unix_time_us()?;
        let header = TraceHeader {
            tool_version: TOOL_VERSION.to_owned(),
            created_unix_us,
            device: metadata,
            report_descriptor,
            descriptor_error,
        };

        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&options.output)
            .with_context(|| {
                format!(
                    "create trace {}; refusing to overwrite an existing file",
                    options.output.display()
                )
            })?;
        let mut writer =
            JsonlTraceWriter::new(BufWriter::new(file), header).context("write trace header")?;

        let running = Arc::new(AtomicBool::new(true));
        let signal_flag = Arc::clone(&running);
        ctrlc::set_handler(move || {
            signal_flag.store(false, Ordering::SeqCst);
        })
        .context("install Ctrl-C handler")?;

        let started = Instant::now();
        let mut buffer = vec![0u8; MAX_INPUT_REPORT_SIZE];
        let mut report_count = 0u64;
        let reason;

        loop {
            if !running.load(Ordering::SeqCst) {
                reason = "ctrl_c";
                break;
            }
            if options
                .max_reports
                .is_some_and(|maximum| report_count >= maximum)
            {
                reason = "max_reports";
                break;
            }
            if options
                .duration
                .is_some_and(|duration| started.elapsed() >= duration)
            {
                reason = "duration";
                break;
            }

            let current_timeout_ms =
                bounded_timeout_ms(timeout_ms, options.duration, started.elapsed());
            if current_timeout_ms == 0 {
                reason = "duration";
                break;
            }
            let byte_count = device
                .read_timeout(&mut buffer, current_timeout_ms)
                .context("read HID input report")?;
            if byte_count == 0 {
                continue;
            }

            let elapsed_us = duration_us(started.elapsed());
            writer
                .write_input_report(unix_time_us()?, elapsed_us, &buffer[..byte_count])
                .context("write input report to trace")?;
            report_count += 1;
        }

        let elapsed_us = duration_us(started.elapsed());
        let written_count = writer
            .finish(unix_time_us()?, elapsed_us, reason)
            .context("write trace footer")?;
        debug_assert_eq!(written_count, report_count);

        Ok(CaptureOutcome {
            output: options.output,
            report_count,
            elapsed_us,
            reason: reason.to_owned(),
        })
    }

    fn matches_filter(device: &DeviceInfo, filter: DeviceFilter) -> bool {
        filter
            .vendor_id
            .map_or(true, |vendor_id| device.vendor_id() == vendor_id)
            && filter
                .product_id
                .map_or(true, |product_id| device.product_id() == product_id)
    }

    fn select_device(api: &HidApi, filter: DeviceFilter, index: usize) -> Result<&DeviceInfo> {
        api.device_list()
            .filter(|device| matches_filter(device, filter))
            .nth(index)
            .with_context(|| {
                format!(
                    "no HID device matched VID {} PID {} at index {index}",
                    format_filter_id(filter.vendor_id),
                    format_filter_id(filter.product_id)
                )
            })
    }

    fn format_filter_id(value: Option<u16>) -> String {
        value
            .map(|value| format!("0x{value:04x}"))
            .unwrap_or_else(|| "*".to_owned())
    }

    fn metadata(
        info: &DeviceInfo,
        filtered_index: usize,
        include_identifiers: bool,
    ) -> DeviceMetadata {
        let (serial_number, path) = redact_identifiers(
            include_identifiers,
            info.serial_number().map(str::to_owned),
            Some(info.path().to_string_lossy().into_owned()),
        );
        DeviceMetadata {
            device_ref: format!("device-{}", filtered_index + 1),
            vendor_id: info.vendor_id(),
            product_id: info.product_id(),
            release_number: info.release_number(),
            usage_page: info.usage_page(),
            usage: info.usage(),
            interface_number: info.interface_number(),
            bus_type: format!("{:?}", info.bus_type()).to_lowercase(),
            manufacturer: info.manufacturer_string().map(str::to_owned),
            product: info.product_string().map(str::to_owned),
            identifiers_redacted: !include_identifiers,
            serial_number,
            path,
        }
    }

    fn redact_identifiers(
        include_identifiers: bool,
        serial_number: Option<String>,
        path: Option<String>,
    ) -> (Option<String>, Option<String>) {
        if include_identifiers {
            (serial_number, path)
        } else {
            (None, None)
        }
    }

    fn read_descriptor(device: &HidDevice) -> Result<DescriptorData> {
        let mut buffer = [0u8; MAX_REPORT_DESCRIPTOR_SIZE];
        let byte_count = device
            .get_report_descriptor(&mut buffer)
            .context("read HID report descriptor through the safe HID API")?;
        DescriptorData::from_bytes(&buffer[..byte_count]).context("parse HID report descriptor")
    }

    fn unix_time_us() -> Result<u64> {
        let micros = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("system clock is before the Unix epoch")?
            .as_micros();
        u64::try_from(micros).context("Unix timestamp exceeds trace format")
    }

    fn duration_us(duration: Duration) -> u64 {
        u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
    }

    fn bounded_timeout_ms(
        configured_ms: i32,
        duration: Option<Duration>,
        elapsed: Duration,
    ) -> i32 {
        let Some(duration) = duration else {
            return configured_ms;
        };
        let Some(remaining) = duration.checked_sub(elapsed) else {
            return 0;
        };
        if remaining.is_zero() {
            return 0;
        }
        let remaining_ms = remaining.as_millis().clamp(1, i32::MAX as u128) as i32;
        configured_ms.min(remaining_ms)
    }

    #[cfg(test)]
    mod tests {
        use super::{bounded_timeout_ms, redact_identifiers};
        use std::time::Duration;

        #[test]
        fn identifiers_are_absent_by_default() {
            let (serial, path) = redact_identifiers(
                false,
                Some("stable-serial".to_owned()),
                Some("stable-path".to_owned()),
            );
            assert_eq!(serial, None);
            assert_eq!(path, None);
        }

        #[test]
        fn timeout_is_bounded_by_remaining_capture_time() {
            assert_eq!(
                bounded_timeout_ms(
                    250,
                    Some(Duration::from_millis(100)),
                    Duration::from_millis(75)
                ),
                25
            );
            assert_eq!(
                bounded_timeout_ms(
                    250,
                    Some(Duration::from_millis(100)),
                    Duration::from_millis(101)
                ),
                0
            );
        }
    }
}

#[cfg(windows)]
pub use platform::{capture_device, describe_device, list_devices};

#[cfg(not(windows))]
pub fn list_devices(
    _filter: DeviceFilter,
    _include_identifiers: bool,
) -> Result<Vec<DeviceMetadata>> {
    bail!("live HID commands are supported only on Windows; offline decode works on this platform")
}

#[cfg(not(windows))]
pub fn describe_device(
    _filter: DeviceFilter,
    _index: usize,
    _include_identifiers: bool,
) -> Result<DeviceDescription> {
    bail!("live HID commands are supported only on Windows; offline decode works on this platform")
}

#[cfg(not(windows))]
pub fn capture_device(_options: CaptureOptions) -> Result<CaptureOutcome> {
    bail!("live HID commands are supported only on Windows; offline decode works on this platform")
}
