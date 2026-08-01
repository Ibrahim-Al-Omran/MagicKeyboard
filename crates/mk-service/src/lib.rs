//! Service orchestration independent of any particular Windows service or HID
//! implementation.

#![forbid(unsafe_code)]

mod console;
mod runtime;

#[cfg(windows)]
pub mod windows;

pub use console::{ConsoleActionSink, ConsoleDeviceSource};
pub use runtime::{
    ActionContext, ActionDispatch, ActionSink, DeviceSource, DriverControl, DriverState, NoopDriver,
    RunSummary, ServiceError, ServiceRuntime, SourceEvent,
};
