use std::io::{BufRead, Write};

use crate::{ActionDispatch, ActionSink, DeviceSource, ServiceError, SourceEvent};

/// Newline-delimited JSON event source for local development and tests.
pub struct ConsoleDeviceSource<R> {
    reader: R,
    line_number: u64,
}

impl<R> ConsoleDeviceSource<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            line_number: 0,
        }
    }

    pub fn into_inner(self) -> R {
        self.reader
    }
}

impl<R: BufRead> DeviceSource for ConsoleDeviceSource<R> {
    fn next_event(&mut self) -> Result<Option<SourceEvent>, ServiceError> {
        loop {
            let mut line = String::new();
            let bytes_read =
                self.reader
                    .read_line(&mut line)
                    .map_err(|source| ServiceError::Io {
                        operation: "read console event",
                        source,
                    })?;
            if bytes_read == 0 {
                return Ok(None);
            }
            self.line_number += 1;

            if line.trim().is_empty() {
                continue;
            }

            return serde_json::from_str(&line)
                .map(Some)
                .map_err(|source| ServiceError::Json {
                    context: format!("reading console event on line {}", self.line_number),
                    source,
                });
        }
    }
}

/// Newline-delimited JSON action output for local development and tests.
pub struct ConsoleActionSink<W> {
    writer: W,
}

impl<W> ConsoleActionSink<W> {
    pub fn new(writer: W) -> Self {
        Self { writer }
    }

    pub fn into_inner(self) -> W {
        self.writer
    }
}

impl<W: Write> ActionSink for ConsoleActionSink<W> {
    fn dispatch(&mut self, dispatch: &ActionDispatch) -> Result<(), ServiceError> {
        let json = serde_json::to_string(dispatch).map_err(|source| ServiceError::Json {
            context: "serializing console action".to_owned(),
            source,
        })?;
        writeln!(self.writer, "{json}")
            .and_then(|()| self.writer.flush())
            .map_err(|source| ServiceError::Io {
                operation: "write console action",
                source,
            })
    }
}

#[cfg(test)]
mod tests {
    use std::io::{BufReader, Cursor};

    use mk_core::{Action, ButtonState, ControlCode, DeviceId, InputEvent};

    use super::*;
    use crate::ActionContext;

    #[test]
    fn console_source_reads_json_lines_and_skips_blanks() {
        let source_event = SourceEvent {
            event: InputEvent::button(
                DeviceId::new("keyboard"),
                7,
                42,
                ControlCode::keyboard(0x04),
                ButtonState::Pressed,
            ),
            device: None,
        };
        let input = format!("\n{}\n", serde_json::to_string(&source_event).unwrap());
        let mut source = ConsoleDeviceSource::new(BufReader::new(Cursor::new(input)));

        assert_eq!(source.next_event().unwrap(), Some(source_event));
        assert_eq!(source.next_event().unwrap(), None);
    }

    #[test]
    fn console_sink_writes_one_json_object_per_line() {
        let dispatch = ActionDispatch {
            context: ActionContext {
                device_id: DeviceId::new("keyboard"),
                sequence: 7,
                profile_id: None,
                matched_rules: Vec::new(),
            },
            action: Action::NoOp,
        };
        let mut sink = ConsoleActionSink::new(Vec::new());

        sink.dispatch(&dispatch).unwrap();
        let output = String::from_utf8(sink.into_inner()).unwrap();

        assert_eq!(output.lines().count(), 1);
        assert_eq!(
            serde_json::from_str::<ActionDispatch>(output.trim()).unwrap(),
            dispatch
        );
    }
}
