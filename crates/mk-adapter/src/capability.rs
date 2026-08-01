use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;

use mk_core::ControlCode;

/// Maximum number of controls retained in either normalized capability set.
pub const MAX_NORMALIZED_CONTROLS: usize = 512;

/// Controls and report bounds exposed only after exact adapter selection.
///
/// Input controls are accepted from the physical adapter. Output controls are
/// operations accepted by a destination adapter (for example a bounded virtual
/// keyboard driver). Report lengths are in bytes and are informational hard
/// bounds for the implementation owning that adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormalizedCapabilities {
    input_controls: BTreeSet<ControlCode>,
    output_controls: BTreeSet<ControlCode>,
    max_input_report_len: u16,
    max_output_report_len: u16,
}

impl NormalizedCapabilities {
    pub fn new(
        input_controls: impl IntoIterator<Item = ControlCode>,
        output_controls: impl IntoIterator<Item = ControlCode>,
        max_input_report_len: u16,
        max_output_report_len: u16,
    ) -> Result<Self, CapabilityError> {
        let input_controls = input_controls.into_iter().collect::<BTreeSet<_>>();
        let output_controls = output_controls.into_iter().collect::<BTreeSet<_>>();

        if input_controls.len() > MAX_NORMALIZED_CONTROLS {
            return Err(CapabilityError::TooManyInputControls {
                actual: input_controls.len(),
                maximum: MAX_NORMALIZED_CONTROLS,
            });
        }
        if output_controls.len() > MAX_NORMALIZED_CONTROLS {
            return Err(CapabilityError::TooManyOutputControls {
                actual: output_controls.len(),
                maximum: MAX_NORMALIZED_CONTROLS,
            });
        }

        Ok(Self {
            input_controls,
            output_controls,
            max_input_report_len,
            max_output_report_len,
        })
    }

    pub fn input_controls(&self) -> &BTreeSet<ControlCode> {
        &self.input_controls
    }

    pub fn output_controls(&self) -> &BTreeSet<ControlCode> {
        &self.output_controls
    }

    pub fn supports_input(&self, control: ControlCode) -> bool {
        self.input_controls.contains(&control)
    }

    pub fn supports_output(&self, control: ControlCode) -> bool {
        self.output_controls.contains(&control)
    }

    pub const fn max_input_report_len(&self) -> u16 {
        self.max_input_report_len
    }

    pub const fn max_output_report_len(&self) -> u16 {
        self.max_output_report_len
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CapabilityError {
    TooManyInputControls { actual: usize, maximum: usize },
    TooManyOutputControls { actual: usize, maximum: usize },
}

impl fmt::Display for CapabilityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManyInputControls { actual, maximum } => write!(
                formatter,
                "input capability count {actual} exceeds maximum {maximum}"
            ),
            Self::TooManyOutputControls { actual, maximum } => write!(
                formatter,
                "output capability count {actual} exceeds maximum {maximum}"
            ),
        }
    }
}

impl Error for CapabilityError {}
