use anyhow::{bail, Context, Result};
use serde::{de::Error as _, Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportLayout {
    pub input_bits: u64,
    pub output_bits: u64,
    pub feature_bits: u64,
    pub input_wire_bytes: u64,
    pub output_wire_bytes: u64,
    pub feature_wire_bytes: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DescriptorSummary {
    pub uses_report_ids: bool,
    #[serde(
        serialize_with = "serialize_report_map",
        deserialize_with = "deserialize_report_map"
    )]
    pub reports: BTreeMap<u8, ReportLayout>,
}

fn serialize_report_map<S>(
    reports: &BTreeMap<u8, ReportLayout>,
    serializer: S,
) -> std::result::Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let string_keys: BTreeMap<String, &ReportLayout> = reports
        .iter()
        .map(|(report_id, layout)| (report_id.to_string(), layout))
        .collect();
    string_keys.serialize(serializer)
}

fn deserialize_report_map<'de, D>(
    deserializer: D,
) -> std::result::Result<BTreeMap<u8, ReportLayout>, D::Error>
where
    D: Deserializer<'de>,
{
    let string_keys = BTreeMap::<String, ReportLayout>::deserialize(deserializer)?;
    let mut reports = BTreeMap::new();
    for (key, layout) in string_keys {
        let report_id = key
            .parse::<u8>()
            .map_err(|_| D::Error::custom(format!("invalid HID report ID key {key:?}")))?;
        if reports.insert(report_id, layout).is_some() {
            return Err(D::Error::custom(format!(
                "duplicate HID report ID key {report_id}"
            )));
        }
    }
    Ok(reports)
}

impl DescriptorSummary {
    pub fn max_input_report_bytes(&self) -> u64 {
        self.reports
            .values()
            .map(|layout| layout.input_wire_bytes)
            .max()
            .unwrap_or(0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DescriptorData {
    pub byte_length: usize,
    pub hex: String,
    pub summary: DescriptorSummary,
}

impl DescriptorData {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        Ok(Self {
            byte_length: bytes.len(),
            hex: hex::encode(bytes),
            summary: summarize_report_descriptor(bytes)?,
        })
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct GlobalState {
    report_size: u32,
    report_count: u32,
    report_id: u8,
}

pub fn summarize_report_descriptor(bytes: &[u8]) -> Result<DescriptorSummary> {
    let mut offset = 0usize;
    let mut state = GlobalState::default();
    let mut stack = Vec::new();
    let mut reports = BTreeMap::<u8, ReportLayout>::new();
    let mut uses_report_ids = false;

    while offset < bytes.len() {
        let prefix = bytes[offset];
        offset += 1;

        if prefix == 0xfe {
            if bytes.len().saturating_sub(offset) < 2 {
                bail!("truncated HID long item header at byte {}", offset - 1);
            }
            let data_length = bytes[offset] as usize;
            offset += 2; // Length and long-item tag.
            if bytes.len().saturating_sub(offset) < data_length {
                bail!("truncated HID long item data at byte {}", offset - 3);
            }
            offset += data_length;
            continue;
        }

        let size = match prefix & 0x03 {
            0 => 0,
            1 => 1,
            2 => 2,
            3 => 4,
            _ => unreachable!(),
        };
        if bytes.len().saturating_sub(offset) < size {
            bail!("truncated HID short item data at byte {}", offset - 1);
        }

        let item_type = (prefix >> 2) & 0x03;
        let tag = (prefix >> 4) & 0x0f;
        let value = read_unsigned(&bytes[offset..offset + size]);
        offset += size;

        match (item_type, tag) {
            // Main: Input, Output, and Feature.
            (0, 8) => add_bits(&mut reports, state, ReportKind::Input)?,
            (0, 9) => add_bits(&mut reports, state, ReportKind::Output)?,
            (0, 11) => add_bits(&mut reports, state, ReportKind::Feature)?,

            // Global: Report Size, Report ID, Report Count, Push, and Pop.
            (1, 7) => state.report_size = value,
            (1, 8) => {
                if value == 0 || value > u8::MAX as u32 {
                    bail!("invalid HID report ID {value}");
                }
                state.report_id = value as u8;
                uses_report_ids = true;
            }
            (1, 9) => state.report_count = value,
            (1, 10) => stack.push(state),
            (1, 11) => {
                state = stack
                    .pop()
                    .context("HID global Pop item has no matching Push")?;
            }
            _ => {}
        }
    }

    if !stack.is_empty() {
        bail!("HID descriptor ended with an unmatched global Push item");
    }

    for layout in reports.values_mut() {
        let report_id_bytes = if uses_report_ids { 1 } else { 0 };
        layout.input_wire_bytes = wire_bytes(layout.input_bits, report_id_bytes);
        layout.output_wire_bytes = wire_bytes(layout.output_bits, report_id_bytes);
        layout.feature_wire_bytes = wire_bytes(layout.feature_bits, report_id_bytes);
    }

    Ok(DescriptorSummary {
        uses_report_ids,
        reports,
    })
}

fn read_unsigned(bytes: &[u8]) -> u32 {
    bytes.iter().enumerate().fold(0u32, |value, (index, byte)| {
        value | ((*byte as u32) << (index * 8))
    })
}

#[derive(Debug, Clone, Copy)]
enum ReportKind {
    Input,
    Output,
    Feature,
}

fn add_bits(
    reports: &mut BTreeMap<u8, ReportLayout>,
    state: GlobalState,
    kind: ReportKind,
) -> Result<()> {
    let bits = u64::from(state.report_size)
        .checked_mul(u64::from(state.report_count))
        .context("HID report bit count overflow")?;
    let layout = reports.entry(state.report_id).or_default();
    let target = match kind {
        ReportKind::Input => &mut layout.input_bits,
        ReportKind::Output => &mut layout.output_bits,
        ReportKind::Feature => &mut layout.feature_bits,
    };
    *target = target
        .checked_add(bits)
        .context("HID accumulated report size overflow")?;
    Ok(())
}

fn wire_bytes(bits: u64, report_id_bytes: u64) -> u64 {
    if bits == 0 {
        0
    } else {
        bits.div_ceil(8) + report_id_bytes
    }
}

#[cfg(test)]
mod tests {
    use super::{summarize_report_descriptor, DescriptorData};

    const KEYBOARD_DESCRIPTOR: &str =
        "05010906a1018501050719e029e71500250175019508810295017508810195067508150025650507190029658100c0";

    #[test]
    fn summarizes_keyboard_input_report() {
        let bytes = hex::decode(KEYBOARD_DESCRIPTOR).unwrap();
        let summary = summarize_report_descriptor(&bytes).unwrap();
        let report = summary.reports.get(&1).unwrap();

        assert!(summary.uses_report_ids);
        assert_eq!(report.input_bits, 64);
        assert_eq!(report.input_wire_bytes, 9);
        assert_eq!(summary.max_input_report_bytes(), 9);
    }

    #[test]
    fn rejects_truncated_items() {
        assert!(summarize_report_descriptor(&[0x76, 0x01]).is_err());
        assert!(summarize_report_descriptor(&[0xfe, 0x04, 0x01, 0x00]).is_err());
    }

    #[test]
    fn descriptor_data_preserves_hex() {
        let descriptor = DescriptorData::from_bytes(&[0x05, 0x01]).unwrap();
        assert_eq!(descriptor.hex, "0501");
        assert_eq!(descriptor.byte_length, 2);
    }
}
