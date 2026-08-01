use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

use mk_core::ControlCode;

use crate::NormalizedCapabilities;

/// Absolute allocation and execution bound for a compiled policy.
pub const MAX_DRIVER_POLICY_OPERATIONS: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OneToOneMapping {
    pub input: ControlCode,
    pub output: ControlCode,
}

impl OneToOneMapping {
    pub const fn new(input: ControlCode, output: ControlCode) -> Self {
        Self { input, output }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PolicyLimits {
    max_operations: usize,
}

impl PolicyLimits {
    pub const MAXIMUM: Self = Self {
        max_operations: MAX_DRIVER_POLICY_OPERATIONS,
    };

    pub fn new(max_operations: usize) -> Result<Self, PolicyLimitError> {
        if max_operations > MAX_DRIVER_POLICY_OPERATIONS {
            return Err(PolicyLimitError {
                requested: max_operations,
                maximum: MAX_DRIVER_POLICY_OPERATIONS,
            });
        }
        Ok(Self { max_operations })
    }

    pub const fn max_operations(self) -> usize {
        self.max_operations
    }
}

impl Default for PolicyLimits {
    fn default() -> Self {
        Self::MAXIMUM
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PolicyLimitError {
    pub requested: usize,
    pub maximum: usize,
}

impl fmt::Display for PolicyLimitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "requested policy bound {} exceeds hard maximum {}",
            self.requested, self.maximum
        )
    }
}

impl Error for PolicyLimitError {}

/// A single primitive understood by the bounded driver policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DriverOperation {
    Remap {
        input: ControlCode,
        output: ControlCode,
    },
}

/// Validated driver representation with no duplicate input or output controls.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DriverPolicy {
    operations: Vec<DriverOperation>,
    operation_bound: usize,
}

impl DriverPolicy {
    pub fn operations(&self) -> &[DriverOperation] {
        &self.operations
    }

    pub fn len(&self) -> usize {
        self.operations.len()
    }

    pub fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }

    pub const fn operation_bound(&self) -> usize {
        self.operation_bound
    }
}

/// Compile mappings only when both endpoints are normalized capabilities.
///
/// The private `DriverPolicy` fields make this the only construction path.
/// Validation occurs before the bounded output vector is allocated.
pub fn compile_policy(
    source: &NormalizedCapabilities,
    destination: &NormalizedCapabilities,
    mappings: &[OneToOneMapping],
    limits: PolicyLimits,
) -> Result<DriverPolicy, CompilePolicyError> {
    if mappings.len() > limits.max_operations {
        return Err(CompilePolicyError::OperationLimitExceeded {
            actual: mappings.len(),
            maximum: limits.max_operations,
        });
    }

    let mut inputs = BTreeMap::new();
    let mut outputs = BTreeMap::new();

    for (index, mapping) in mappings.iter().enumerate() {
        if !source.supports_input(mapping.input) {
            return Err(CompilePolicyError::UnsupportedInput {
                index,
                control: mapping.input,
            });
        }
        if !destination.supports_output(mapping.output) {
            return Err(CompilePolicyError::UnsupportedOutput {
                index,
                control: mapping.output,
            });
        }
        if let Some(first_index) = inputs.insert(mapping.input, index) {
            return Err(CompilePolicyError::InputConflict {
                control: mapping.input,
                first_index,
                second_index: index,
            });
        }
        if let Some(first_index) = outputs.insert(mapping.output, index) {
            return Err(CompilePolicyError::OutputConflict {
                control: mapping.output,
                first_index,
                second_index: index,
            });
        }
    }

    let operations = mappings
        .iter()
        .map(|mapping| DriverOperation::Remap {
            input: mapping.input,
            output: mapping.output,
        })
        .collect();

    Ok(DriverPolicy {
        operations,
        operation_bound: limits.max_operations,
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CompilePolicyError {
    OperationLimitExceeded {
        actual: usize,
        maximum: usize,
    },
    UnsupportedInput {
        index: usize,
        control: ControlCode,
    },
    UnsupportedOutput {
        index: usize,
        control: ControlCode,
    },
    InputConflict {
        control: ControlCode,
        first_index: usize,
        second_index: usize,
    },
    OutputConflict {
        control: ControlCode,
        first_index: usize,
        second_index: usize,
    },
}

impl fmt::Display for CompilePolicyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OperationLimitExceeded { actual, maximum } => write!(
                formatter,
                "mapping count {actual} exceeds policy operation bound {maximum}"
            ),
            Self::UnsupportedInput { index, control } => write!(
                formatter,
                "mapping {index} uses unsupported input {:04x}:{:04x}",
                control.usage_page, control.usage
            ),
            Self::UnsupportedOutput { index, control } => write!(
                formatter,
                "mapping {index} uses unsupported output {:04x}:{:04x}",
                control.usage_page, control.usage
            ),
            Self::InputConflict {
                control,
                first_index,
                second_index,
            } => write!(
                formatter,
                "input {:04x}:{:04x} is used by mappings {first_index} and {second_index}",
                control.usage_page, control.usage
            ),
            Self::OutputConflict {
                control,
                first_index,
                second_index,
            } => write!(
                formatter,
                "output {:04x}:{:04x} is used by mappings {first_index} and {second_index}",
                control.usage_page, control.usage
            ),
        }
    }
}

impl Error for CompilePolicyError {}
