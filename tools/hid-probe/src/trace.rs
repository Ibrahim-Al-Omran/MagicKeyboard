use crate::descriptor::DescriptorData;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{BufRead, Write};

pub const TRACE_SCHEMA: &str = "hid-probe.trace";
pub const TRACE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceMetadata {
    pub device_ref: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub release_number: u16,
    pub usage_page: u16,
    pub usage: u16,
    pub interface_number: i32,
    pub bus_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manufacturer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product: Option<String>,
    pub identifiers_redacted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serial_number: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceHeader {
    pub tool_version: String,
    pub created_unix_us: u64,
    pub device: DeviceMetadata,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report_descriptor: Option<DescriptorData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub descriptor_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "record_type", rename_all = "snake_case")]
pub enum TraceEvent {
    Header(TraceHeader),
    InputReport {
        sequence: u64,
        timestamp_unix_us: u64,
        elapsed_us: u64,
        byte_length: usize,
        data_hex: String,
    },
    Footer {
        ended_unix_us: u64,
        elapsed_us: u64,
        report_count: u64,
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceEnvelope {
    pub schema: String,
    pub schema_version: u32,
    #[serde(flatten)]
    pub event: TraceEvent,
}

impl TraceEnvelope {
    pub fn new(event: TraceEvent) -> Self {
        Self {
            schema: TRACE_SCHEMA.to_owned(),
            schema_version: TRACE_SCHEMA_VERSION,
            event,
        }
    }
}

pub struct JsonlTraceWriter<W: Write> {
    writer: W,
    next_sequence: u64,
}

impl<W: Write> JsonlTraceWriter<W> {
    pub fn new(mut writer: W, header: TraceHeader) -> Result<Self> {
        write_envelope(&mut writer, &TraceEnvelope::new(TraceEvent::Header(header)))?;
        Ok(Self {
            writer,
            next_sequence: 0,
        })
    }

    pub fn write_input_report(
        &mut self,
        timestamp_unix_us: u64,
        elapsed_us: u64,
        data: &[u8],
    ) -> Result<()> {
        let event = TraceEvent::InputReport {
            sequence: self.next_sequence,
            timestamp_unix_us,
            elapsed_us,
            byte_length: data.len(),
            data_hex: hex::encode(data),
        };
        write_envelope(&mut self.writer, &TraceEnvelope::new(event))?;
        self.next_sequence += 1;
        Ok(())
    }

    pub fn finish(
        mut self,
        ended_unix_us: u64,
        elapsed_us: u64,
        reason: impl Into<String>,
    ) -> Result<u64> {
        let report_count = self.next_sequence;
        let event = TraceEvent::Footer {
            ended_unix_us,
            elapsed_us,
            report_count,
            reason: reason.into(),
        };
        write_envelope(&mut self.writer, &TraceEnvelope::new(event))?;
        Ok(report_count)
    }
}

fn write_envelope(writer: &mut impl Write, envelope: &TraceEnvelope) -> Result<()> {
    serde_json::to_writer(&mut *writer, envelope).context("serialize JSONL trace record")?;
    writer.write_all(b"\n").context("write JSONL newline")?;
    writer.flush().context("flush JSONL trace")?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputReport {
    pub sequence: u64,
    pub timestamp_unix_us: u64,
    pub elapsed_us: u64,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceFooter {
    pub ended_unix_us: u64,
    pub elapsed_us: u64,
    pub report_count: u64,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trace {
    pub header: TraceHeader,
    pub reports: Vec<InputReport>,
    pub footer: Option<TraceFooter>,
}

pub fn read_trace(reader: impl BufRead) -> Result<Trace> {
    let mut header = None;
    let mut reports = Vec::new();
    let mut footer = None;

    for (line_index, line) in reader.lines().enumerate() {
        let line_number = line_index + 1;
        let line = line.with_context(|| format!("read trace line {line_number}"))?;
        if line.trim().is_empty() {
            bail!("trace line {line_number} is empty");
        }

        let envelope: TraceEnvelope = serde_json::from_str(&line)
            .with_context(|| format!("parse JSON on trace line {line_number}"))?;
        if envelope.schema != TRACE_SCHEMA {
            bail!(
                "unsupported trace schema {:?} on line {line_number}",
                envelope.schema
            );
        }
        if envelope.schema_version != TRACE_SCHEMA_VERSION {
            bail!(
                "unsupported trace schema version {} on line {line_number}",
                envelope.schema_version
            );
        }

        match envelope.event {
            TraceEvent::Header(value) => {
                if line_number != 1 || header.is_some() {
                    bail!("trace header must be the first and only header record");
                }
                validate_header(&value)
                    .with_context(|| format!("validate trace header on line {line_number}"))?;
                header = Some(value);
            }
            TraceEvent::InputReport {
                sequence,
                timestamp_unix_us,
                elapsed_us,
                byte_length,
                data_hex,
            } => {
                if header.is_none() {
                    bail!("input report precedes trace header on line {line_number}");
                }
                if footer.is_some() {
                    bail!("input report follows trace footer on line {line_number}");
                }
                if sequence != reports.len() as u64 {
                    bail!(
                        "unexpected report sequence {sequence} on line {line_number}; expected {}",
                        reports.len()
                    );
                }
                if reports
                    .last()
                    .is_some_and(|previous: &InputReport| previous.elapsed_us > elapsed_us)
                {
                    bail!("report elapsed_us moved backwards on line {line_number}");
                }
                let data = hex::decode(&data_hex)
                    .with_context(|| format!("decode report hex on trace line {line_number}"))?;
                if data.len() != byte_length {
                    bail!(
                        "report byte_length {byte_length} does not match {} decoded bytes on line {line_number}",
                        data.len()
                    );
                }
                reports.push(InputReport {
                    sequence,
                    timestamp_unix_us,
                    elapsed_us,
                    data,
                });
            }
            TraceEvent::Footer {
                ended_unix_us,
                elapsed_us,
                report_count,
                reason,
            } => {
                if header.is_none() {
                    bail!("trace footer precedes trace header on line {line_number}");
                }
                if footer.is_some() {
                    bail!("trace has more than one footer");
                }
                if report_count != reports.len() as u64 {
                    bail!(
                        "footer report_count {report_count} does not match {} reports",
                        reports.len()
                    );
                }
                if reports
                    .last()
                    .is_some_and(|report| report.elapsed_us > elapsed_us)
                {
                    bail!("footer elapsed_us precedes the final input report");
                }
                footer = Some(TraceFooter {
                    ended_unix_us,
                    elapsed_us,
                    report_count,
                    reason,
                });
            }
        }
    }

    let header = header.context("trace is empty or has no header")?;
    Ok(Trace {
        header,
        reports,
        footer,
    })
}

fn validate_header(header: &TraceHeader) -> Result<()> {
    if header.device.identifiers_redacted
        && (header.device.serial_number.is_some() || header.device.path.is_some())
    {
        bail!("header claims identifier redaction but contains a serial number or device path");
    }

    if let Some(descriptor) = &header.report_descriptor {
        let bytes = hex::decode(&descriptor.hex).context("decode report descriptor hex")?;
        if bytes.len() != descriptor.byte_length {
            bail!(
                "descriptor byte_length {} does not match {} decoded bytes",
                descriptor.byte_length,
                bytes.len()
            );
        }
        let parsed =
            DescriptorData::from_bytes(&bytes).context("parse embedded report descriptor")?;
        if parsed.summary != descriptor.summary {
            bail!("embedded report descriptor summary does not match its raw bytes");
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TraceSummary {
    pub schema_version: u32,
    pub device_ref: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub report_count: usize,
    pub capture_complete: bool,
    pub duration_us: u64,
    pub average_reports_per_second: f64,
    pub length_counts: BTreeMap<usize, u64>,
    pub report_id_counts: BTreeMap<String, u64>,
    pub changed_byte_positions: BTreeMap<usize, u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub footer_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DecodedReport {
    pub sequence: u64,
    pub timestamp_unix_us: u64,
    pub elapsed_us: u64,
    pub byte_length: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report_id: Option<u8>,
    pub payload_hex: String,
}

impl Trace {
    pub fn uses_report_ids(&self) -> bool {
        self.header
            .report_descriptor
            .as_ref()
            .is_some_and(|descriptor| descriptor.summary.uses_report_ids)
    }

    pub fn decoded_reports(&self) -> Vec<DecodedReport> {
        let uses_report_ids = self.uses_report_ids();
        self.reports
            .iter()
            .map(|report| {
                let (report_id, payload) = if uses_report_ids {
                    (
                        report.data.first().copied(),
                        report.data.get(1..).unwrap_or(&[]),
                    )
                } else {
                    (None, report.data.as_slice())
                };
                DecodedReport {
                    sequence: report.sequence,
                    timestamp_unix_us: report.timestamp_unix_us,
                    elapsed_us: report.elapsed_us,
                    byte_length: report.data.len(),
                    report_id,
                    payload_hex: hex::encode(payload),
                }
            })
            .collect()
    }

    pub fn summary(&self) -> TraceSummary {
        let uses_report_ids = self.uses_report_ids();
        let mut length_counts = BTreeMap::new();
        let mut report_id_counts = BTreeMap::new();
        let mut changed_byte_positions = BTreeMap::new();
        let mut previous_by_id = BTreeMap::<Option<u8>, Vec<u8>>::new();

        for report in &self.reports {
            *length_counts.entry(report.data.len()).or_default() += 1;
            let report_id = if uses_report_ids {
                report.data.first().copied()
            } else {
                None
            };
            let id_label = report_id
                .map(|id| format!("0x{id:02x}"))
                .unwrap_or_else(|| "none".to_owned());
            *report_id_counts.entry(id_label).or_default() += 1;

            if let Some(previous) = previous_by_id.get(&report_id) {
                for index in 0..previous.len().max(report.data.len()) {
                    if previous.get(index) != report.data.get(index) {
                        *changed_byte_positions.entry(index).or_default() += 1;
                    }
                }
            }
            previous_by_id.insert(report_id, report.data.clone());
        }

        let duration_us = self
            .footer
            .as_ref()
            .map(|footer| footer.elapsed_us)
            .or_else(|| self.reports.last().map(|report| report.elapsed_us))
            .unwrap_or(0);
        let average_reports_per_second = if duration_us == 0 {
            0.0
        } else {
            self.reports.len() as f64 * 1_000_000.0 / duration_us as f64
        };

        TraceSummary {
            schema_version: TRACE_SCHEMA_VERSION,
            device_ref: self.header.device.device_ref.clone(),
            vendor_id: self.header.device.vendor_id,
            product_id: self.header.device.product_id,
            report_count: self.reports.len(),
            capture_complete: self.footer.is_some(),
            duration_us,
            average_reports_per_second,
            length_counts,
            report_id_counts,
            changed_byte_positions,
            footer_reason: self.footer.as_ref().map(|footer| footer.reason.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{read_trace, DeviceMetadata, JsonlTraceWriter, TraceHeader};
    use crate::descriptor::DescriptorData;
    use std::io::{BufReader, Cursor};

    fn test_header() -> TraceHeader {
        TraceHeader {
            tool_version: "test".to_owned(),
            created_unix_us: 1_700_000_000_000_000,
            device: DeviceMetadata {
                device_ref: "device-1".to_owned(),
                vendor_id: 0x05ac,
                product_id: 0x0267,
                release_number: 1,
                usage_page: 1,
                usage: 6,
                interface_number: 0,
                bus_type: "usb".to_owned(),
                manufacturer: Some("Apple Inc.".to_owned()),
                product: Some("Magic Keyboard".to_owned()),
                identifiers_redacted: true,
                serial_number: None,
                path: None,
            },
            report_descriptor: Some(
                DescriptorData::from_bytes(&[0x85, 0x01, 0x75, 0x08, 0x95, 0x02, 0x81, 0x02])
                    .unwrap(),
            ),
            descriptor_error: None,
        }
    }

    #[test]
    fn writer_round_trips_and_summarizes() {
        let mut output = Vec::new();
        {
            let mut writer = JsonlTraceWriter::new(&mut output, test_header()).unwrap();
            writer
                .write_input_report(1_700_000_000_000_100, 100, &[1, 0, 4])
                .unwrap();
            writer
                .write_input_report(1_700_000_000_001_100, 1_100, &[1, 0, 5])
                .unwrap();
            writer.finish(1_700_000_000_001_200, 1_200, "test").unwrap();
        }

        let trace = read_trace(BufReader::new(Cursor::new(output))).unwrap();
        let summary = trace.summary();
        assert_eq!(summary.report_count, 2);
        assert_eq!(summary.report_id_counts.get("0x01"), Some(&2));
        assert_eq!(summary.changed_byte_positions.get(&2), Some(&1));
        assert!(summary.capture_complete);
    }

    #[test]
    fn rejects_mismatched_declared_length() {
        let input = r#"{"schema":"hid-probe.trace","schema_version":1,"record_type":"header","tool_version":"test","created_unix_us":1,"device":{"device_ref":"device-1","vendor_id":1452,"product_id":615,"release_number":1,"usage_page":1,"usage":6,"interface_number":0,"bus_type":"usb","identifiers_redacted":true}}
{"schema":"hid-probe.trace","schema_version":1,"record_type":"input_report","sequence":0,"timestamp_unix_us":2,"elapsed_us":1,"byte_length":2,"data_hex":"00"}
"#;
        assert!(read_trace(BufReader::new(Cursor::new(input))).is_err());
    }

    #[test]
    fn rejects_identifiers_in_a_redacted_header() {
        let mut header = test_header();
        header.device.serial_number = Some("should-not-be-present".to_owned());
        let mut output = Vec::new();
        JsonlTraceWriter::new(&mut output, header).unwrap();

        let error = read_trace(BufReader::new(Cursor::new(output))).unwrap_err();
        assert!(error.to_string().contains("validate trace header"));
    }
}
