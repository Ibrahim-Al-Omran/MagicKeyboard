use std::error::Error as StdError;
use std::io;

use mk_core::{
    Action, DeviceId, DeviceIdentity, EngineError, InputEvent, MappingEngine, ProfileId, RuleId,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error("failed to {operation}: {source}")]
    Io {
        operation: &'static str,
        #[source]
        source: io::Error,
    },
    #[error("invalid JSON while {context}: {source}")]
    Json {
        context: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("{component} adapter failed: {message}")]
    Adapter {
        component: &'static str,
        message: String,
        #[source]
        source: Option<Box<dyn StdError + Send + Sync>>,
    },
    #[error(
        "event device '{}' does not match descriptor device '{}'",
        event_device,
        descriptor_device
    )]
    DeviceIdentityMismatch {
        event_device: DeviceId,
        descriptor_device: DeviceId,
    },
    #[error(transparent)]
    Engine(#[from] EngineError),
}

impl ServiceError {
    pub fn adapter(component: &'static str, message: impl Into<String>) -> Self {
        Self::Adapter {
            component,
            message: message.into(),
            source: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SourceEvent {
    pub event: InputEvent,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<DeviceIdentity>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ActionContext {
    pub device_id: DeviceId,
    pub sequence: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<ProfileId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub matched_rules: Vec<RuleId>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ActionDispatch {
    pub context: ActionContext,
    pub action: Action,
}

pub trait DeviceSource {
    /// Return `Ok(None)` when a finite source is exhausted.
    fn next_event(&mut self) -> Result<Option<SourceEvent>, ServiceError>;
}

pub trait ActionSink {
    fn dispatch(&mut self, dispatch: &ActionDispatch) -> Result<(), ServiceError>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DriverState {
    Stopped,
    Running,
}

pub trait DriverControl {
    fn state(&self) -> DriverState;
    fn start(&mut self) -> Result<(), ServiceError>;
    fn stop(&mut self) -> Result<(), ServiceError>;
}

#[derive(Debug, Default)]
pub struct NoopDriver {
    running: bool,
}

impl DriverControl for NoopDriver {
    fn state(&self) -> DriverState {
        if self.running {
            DriverState::Running
        } else {
            DriverState::Stopped
        }
    }

    fn start(&mut self) -> Result<(), ServiceError> {
        self.running = true;
        Ok(())
    }

    fn stop(&mut self) -> Result<(), ServiceError> {
        self.running = false;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RunSummary {
    pub events_processed: u64,
    pub actions_dispatched: u64,
}

pub struct ServiceRuntime<S, A, D> {
    engine: MappingEngine,
    source: S,
    sink: A,
    driver: D,
}

impl<S, A, D> ServiceRuntime<S, A, D>
where
    S: DeviceSource,
    A: ActionSink,
    D: DriverControl,
{
    pub fn new(engine: MappingEngine, source: S, sink: A, driver: D) -> Self {
        Self {
            engine,
            source,
            sink,
            driver,
        }
    }

    pub fn run_to_end(&mut self) -> Result<RunSummary, ServiceError> {
        if self.engine.config().service.driver_enabled {
            self.ensure_driver(true)?;
        }

        let mut summary = RunSummary::default();
        let processing_result = loop {
            match self.process_next() {
                Ok(Some(action_count)) => {
                    summary.events_processed += 1;
                    summary.actions_dispatched += action_count as u64;
                }
                Ok(None) => break Ok(summary),
                Err(error) => break Err(error),
            }
        };

        let shutdown_result = self.ensure_driver(false);
        match (processing_result, shutdown_result) {
            (Err(processing_error), _) => Err(processing_error),
            (Ok(_), Err(shutdown_error)) => Err(shutdown_error),
            (Ok(summary), Ok(())) => Ok(summary),
        }
    }

    /// Process one source event, returning its dispatched action count.
    pub fn process_next(&mut self) -> Result<Option<usize>, ServiceError> {
        let Some(source_event) = self.source.next_event()? else {
            return Ok(None);
        };

        if let Some(device) = &source_event.device {
            if device.id != source_event.event.device_id {
                return Err(ServiceError::DeviceIdentityMismatch {
                    event_device: source_event.event.device_id,
                    descriptor_device: device.id.clone(),
                });
            }
        }

        let output = self
            .engine
            .process(&source_event.event, source_event.device.as_ref());
        let context = ActionContext {
            device_id: source_event.event.device_id,
            sequence: source_event.event.sequence,
            profile_id: output.profile_id,
            matched_rules: output.matched_rules,
        };
        let action_count = output.actions.len();

        for action in output.actions {
            if let Action::SetDriverEnabled { enabled } = &action {
                self.ensure_driver(*enabled)?;
            }
            self.sink.dispatch(&ActionDispatch {
                context: context.clone(),
                action,
            })?;
        }

        Ok(Some(action_count))
    }

    pub fn engine(&self) -> &MappingEngine {
        &self.engine
    }

    pub fn sink(&self) -> &A {
        &self.sink
    }

    pub fn driver(&self) -> &D {
        &self.driver
    }

    pub fn into_parts(self) -> (MappingEngine, S, A, D) {
        (self.engine, self.source, self.sink, self.driver)
    }

    fn ensure_driver(&mut self, enabled: bool) -> Result<(), ServiceError> {
        match (enabled, self.driver.state()) {
            (true, DriverState::Stopped) => self.driver.start(),
            (false, DriverState::Running) => self.driver.stop(),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use mk_core::{
        ButtonState, ConfigDocument, ControlCode, DeviceId, MappingRule, Profile, ProfileId,
        Trigger, CURRENT_SCHEMA_VERSION,
    };

    use super::*;

    struct QueueSource(VecDeque<SourceEvent>);

    impl DeviceSource for QueueSource {
        fn next_event(&mut self) -> Result<Option<SourceEvent>, ServiceError> {
            Ok(self.0.pop_front())
        }
    }

    #[derive(Default)]
    struct CollectSink(Vec<ActionDispatch>);

    impl ActionSink for CollectSink {
        fn dispatch(&mut self, dispatch: &ActionDispatch) -> Result<(), ServiceError> {
            self.0.push(dispatch.clone());
            Ok(())
        }
    }

    fn test_engine() -> MappingEngine {
        let config = ConfigDocument {
            schema_version: CURRENT_SCHEMA_VERSION,
            profiles: vec![Profile {
                id: ProfileId::new("default"),
                name: None,
                selectors: Vec::new(),
                selection_priority: 0,
                mappings: vec![MappingRule {
                    id: RuleId::new("disable-driver"),
                    enabled: true,
                    priority: 0,
                    trigger: Trigger::button(
                        ControlCode::keyboard(0x04),
                        ButtonState::Pressed,
                    ),
                    actions: vec![Action::SetDriverEnabled { enabled: false }],
                    consume: true,
                }],
            }],
            default_profile: Some(ProfileId::new("default")),
            ..ConfigDocument::default()
        };
        MappingEngine::new(config).unwrap()
    }

    #[test]
    fn orchestrates_source_engine_sink_and_driver() {
        let source = QueueSource(VecDeque::from([SourceEvent {
            event: InputEvent::button(
                DeviceId::new("keyboard"),
                1,
                10,
                ControlCode::keyboard(0x04),
                ButtonState::Pressed,
            ),
            device: None,
        }]));
        let mut runtime =
            ServiceRuntime::new(test_engine(), source, CollectSink::default(), NoopDriver::default());

        let summary = runtime.run_to_end().unwrap();

        assert_eq!(
            summary,
            RunSummary {
                events_processed: 1,
                actions_dispatched: 1
            }
        );
        assert_eq!(runtime.sink().0.len(), 1);
        assert_eq!(runtime.driver().state(), DriverState::Stopped);
    }
}
