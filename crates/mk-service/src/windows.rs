//! Windows integration seams.
//!
//! These adapters deliberately fail with structured "not implemented" errors
//! until a reviewed service-control and HID backend is added. No device paths,
//! report layouts, or driver capabilities are assumed here.

use crate::{DeviceSource, DriverControl, DriverState, ServiceError, SourceEvent};

#[derive(Debug, Default)]
pub struct WindowsHidSource;

impl DeviceSource for WindowsHidSource {
    fn next_event(&mut self) -> Result<Option<SourceEvent>, ServiceError> {
        Err(ServiceError::adapter(
            "windows HID",
            "native HID input is not implemented; use --console",
        ))
    }
}

#[derive(Debug, Default)]
pub struct WindowsDriverControl {
    running: bool,
}

impl DriverControl for WindowsDriverControl {
    fn state(&self) -> DriverState {
        if self.running {
            DriverState::Running
        } else {
            DriverState::Stopped
        }
    }

    fn start(&mut self) -> Result<(), ServiceError> {
        Err(ServiceError::adapter(
            "windows driver",
            "native driver control is not implemented; use the console no-op driver",
        ))
    }

    fn stop(&mut self) -> Result<(), ServiceError> {
        self.running = false;
        Ok(())
    }
}
