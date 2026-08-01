use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use hid_probe::hardware::{
    capture_device, describe_device, list_devices, CaptureOptions, DeviceFilter,
};
use hid_probe::parse_hex_u16;
use hid_probe::trace::{read_trace, DecodedReport, TraceSummary};
use serde::Serialize;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Parser)]
#[command(
    name = "hid-probe",
    version,
    about = "Read-only Windows HID report recorder and offline analyzer",
    long_about = "Enumerate and record HID input reports without sending output or feature reports. Stable identifiers are redacted unless explicitly requested."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Enumerate HID interfaces and safe metadata.
    #[command(visible_alias = "enumerate")]
    List {
        #[command(flatten)]
        filter: FilterArgs,
        /// Include serial numbers and OS device paths in output.
        #[arg(long)]
        include_identifiers: bool,
        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Open one matched interface and inspect its report descriptor.
    Describe {
        #[command(flatten)]
        filter: FilterArgs,
        /// Zero-based index within the filtered list.
        #[arg(long, default_value_t = 0)]
        index: usize,
        /// Include serial numbers and OS device paths in output.
        #[arg(long)]
        include_identifiers: bool,
        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Record only unsolicited HID input reports to a new JSONL file.
    Capture {
        /// USB vendor ID in hexadecimal.
        #[arg(long, value_parser = parse_hex_u16)]
        vid: u16,
        /// USB product ID in hexadecimal.
        #[arg(long, value_parser = parse_hex_u16)]
        pid: u16,
        /// Zero-based index within the matching VID/PID interfaces.
        #[arg(long, default_value_t = 0)]
        index: usize,
        /// New JSONL trace path; existing files are never overwritten.
        #[arg(long)]
        output: PathBuf,
        /// Stop after this many seconds.
        #[arg(long, value_parser = parse_positive_u64)]
        duration_seconds: Option<u64>,
        /// Stop after this many input reports.
        #[arg(long, value_parser = parse_positive_u64)]
        max_reports: Option<u64>,
        /// Poll timeout used to notice Ctrl-C and duration expiry.
        #[arg(long, default_value_t = 250, value_parser = parse_positive_u64)]
        read_timeout_ms: u64,
        /// Store serial number and OS device path in the trace header.
        #[arg(long)]
        include_identifiers: bool,
    },
    /// Validate and generically decode/summarize a JSONL trace offline.
    #[command(visible_alias = "summarize")]
    Decode {
        /// JSONL trace captured by this tool.
        input: PathBuf,
        /// Include each generic report decode after the summary.
        #[arg(long)]
        reports: bool,
        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Clone, Copy, Args)]
struct FilterArgs {
    /// USB vendor ID in hexadecimal (for example 05ac).
    #[arg(long, value_parser = parse_hex_u16)]
    vid: Option<u16>,
    /// USB product ID in hexadecimal.
    #[arg(long, value_parser = parse_hex_u16)]
    pid: Option<u16>,
}

impl From<FilterArgs> for DeviceFilter {
    fn from(value: FilterArgs) -> Self {
        Self {
            vendor_id: value.vid,
            product_id: value.pid,
        }
    }
}

#[derive(Debug, Serialize)]
struct DecodeOutput {
    summary: TraceSummary,
    #[serde(skip_serializing_if = "Option::is_none")]
    reports: Option<Vec<DecodedReport>>,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    match Cli::parse().command {
        Command::List {
            filter,
            include_identifiers,
            json,
        } => {
            identifier_warning(include_identifiers);
            let devices = list_devices(filter.into(), include_identifiers)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&devices)?);
            } else {
                println!("matched HID interfaces: {}", devices.len());
                for (index, device) in devices.iter().enumerate() {
                    println!(
                        "[{index}] {:04x}:{:04x} bus={} usage={:04x}:{:04x} interface={} product={}",
                        device.vendor_id,
                        device.product_id,
                        device.bus_type,
                        device.usage_page,
                        device.usage,
                        device.interface_number,
                        device.product.as_deref().unwrap_or("<unavailable>")
                    );
                    if include_identifiers {
                        println!(
                            "    serial={} path={}",
                            device.serial_number.as_deref().unwrap_or("<unavailable>"),
                            device.path.as_deref().unwrap_or("<unavailable>")
                        );
                    }
                }
                if !include_identifiers {
                    println!("stable identifiers: redacted (default)");
                }
            }
        }
        Command::Describe {
            filter,
            index,
            include_identifiers,
            json,
        } => {
            identifier_warning(include_identifiers);
            let description = describe_device(filter.into(), index, include_identifiers)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&description)?);
            } else {
                let device = &description.device;
                let descriptor = &description.report_descriptor;
                println!(
                    "{} {:04x}:{:04x} bus={} product={}",
                    device.device_ref,
                    device.vendor_id,
                    device.product_id,
                    device.bus_type,
                    device.product.as_deref().unwrap_or("<unavailable>")
                );
                println!(
                    "descriptor: {} bytes, report_ids={}, max_input_wire_bytes={}",
                    descriptor.byte_length,
                    descriptor.summary.uses_report_ids,
                    descriptor.summary.max_input_report_bytes()
                );
                for (report_id, layout) in &descriptor.summary.reports {
                    println!(
                        "  report 0x{report_id:02x}: input={}b/{}B output={}b/{}B feature={}b/{}B",
                        layout.input_bits,
                        layout.input_wire_bytes,
                        layout.output_bits,
                        layout.output_wire_bytes,
                        layout.feature_bits,
                        layout.feature_wire_bytes
                    );
                }
                println!("descriptor_hex={}", descriptor.hex);
                if !include_identifiers {
                    println!("stable identifiers: redacted (default)");
                }
            }
        }
        Command::Capture {
            vid,
            pid,
            index,
            output,
            duration_seconds,
            max_reports,
            read_timeout_ms,
            include_identifiers,
        } => {
            identifier_warning(include_identifiers);
            let outcome = capture_device(CaptureOptions {
                filter: DeviceFilter {
                    vendor_id: Some(vid),
                    product_id: Some(pid),
                },
                index,
                output,
                duration: duration_seconds.map(Duration::from_secs),
                max_reports,
                read_timeout: Duration::from_millis(read_timeout_ms),
                include_identifiers,
            })?;
            println!(
                "captured {} input reports to {} in {} us (reason: {})",
                outcome.report_count,
                outcome.output.display(),
                outcome.elapsed_us,
                outcome.reason
            );
        }
        Command::Decode {
            input,
            reports,
            json,
        } => decode(&input, reports, json)?,
    }
    Ok(())
}

fn decode(path: &Path, include_reports: bool, json: bool) -> Result<()> {
    let file = File::open(path).with_context(|| format!("open trace {}", path.display()))?;
    let trace =
        read_trace(BufReader::new(file)).with_context(|| format!("decode {}", path.display()))?;
    let summary = trace.summary();

    if json {
        let output = DecodeOutput {
            summary,
            reports: include_reports.then(|| trace.decoded_reports()),
        };
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    println!(
        "device: {} {:04x}:{:04x}",
        summary.device_ref, summary.vendor_id, summary.product_id
    );
    println!(
        "reports: {} complete={} duration_us={} average_reports_per_second={:.3}",
        summary.report_count,
        summary.capture_complete,
        summary.duration_us,
        summary.average_reports_per_second
    );
    println!("length_counts: {:?}", summary.length_counts);
    println!("report_id_counts: {:?}", summary.report_id_counts);
    println!(
        "changed_byte_positions: {:?}",
        summary.changed_byte_positions
    );
    if let Some(reason) = &summary.footer_reason {
        println!("footer_reason: {reason}");
    }

    if include_reports {
        for report in trace.decoded_reports() {
            let report_id = report
                .report_id
                .map(|id| format!("0x{id:02x}"))
                .unwrap_or_else(|| "none".to_owned());
            println!(
                "seq={} elapsed_us={} len={} report_id={} payload={}",
                report.sequence,
                report.elapsed_us,
                report.byte_length,
                report_id,
                report.payload_hex
            );
        }
    }
    Ok(())
}

fn identifier_warning(include_identifiers: bool) {
    if include_identifiers {
        eprintln!(
            "warning: --include-identifiers exposes stable serial numbers and OS device paths"
        );
    }
}

fn parse_positive_u64(value: &str) -> std::result::Result<u64, String> {
    let value = value
        .parse::<u64>()
        .map_err(|_| format!("expected a positive integer, got {value:?}"))?;
    if value == 0 {
        return Err("value must be greater than zero".to_owned());
    }
    Ok(value)
}
