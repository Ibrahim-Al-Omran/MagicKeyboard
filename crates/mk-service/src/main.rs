#![forbid(unsafe_code)]

use std::env;
use std::ffi::OsString;
use std::io;
use std::path::PathBuf;
use std::process::ExitCode;

use mk_core::{load_config, EngineError, MappingEngine, PersistenceError};
use mk_service::{
    ConsoleActionSink, ConsoleDeviceSource, NoopDriver, ServiceError, ServiceRuntime,
};
use thiserror::Error;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("mk-service: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), AppError> {
    let options = match Options::parse(env::args_os().skip(1))? {
        ParsedOptions::Help => {
            print_help();
            return Ok(());
        }
        ParsedOptions::Run(options) => options,
    };

    if !options.console {
        return Err(AppError::ConsoleModeRequired);
    }

    let config = load_config(&options.config)?;
    let engine = MappingEngine::new(config)?;
    let stdin = io::stdin();
    let stdout = io::stdout();
    let source = ConsoleDeviceSource::new(stdin.lock());
    let sink = ConsoleActionSink::new(stdout.lock());
    let mut runtime = ServiceRuntime::new(engine, source, sink, NoopDriver::default());
    let summary = runtime.run_to_end()?;
    eprintln!(
        "processed {} event(s), dispatched {} action(s)",
        summary.events_processed, summary.actions_dispatched
    );
    Ok(())
}

fn print_help() {
    println!(
        "Usage: mk-service --console [--config PATH]\n\
         \n\
         Reads newline-delimited SourceEvent JSON from stdin and writes\n\
         newline-delimited ActionDispatch JSON to stdout.\n\
         The default config path is magic-keyboard.json."
    );
}

struct Options {
    config: PathBuf,
    console: bool,
}

enum ParsedOptions {
    Help,
    Run(Options),
}

impl Options {
    fn parse(arguments: impl IntoIterator<Item = OsString>) -> Result<ParsedOptions, AppError> {
        let mut arguments = arguments.into_iter();
        let mut options = Self {
            config: PathBuf::from("magic-keyboard.json"),
            console: false,
        };

        while let Some(argument) = arguments.next() {
            match argument.to_str() {
                Some("--console") => options.console = true,
                Some("--config") => {
                    options.config = arguments
                        .next()
                        .map(PathBuf::from)
                        .ok_or(AppError::MissingConfigPath)?;
                }
                Some("--help" | "-h") => return Ok(ParsedOptions::Help),
                _ => return Err(AppError::UnknownArgument(argument)),
            }
        }

        Ok(ParsedOptions::Run(options))
    }
}

#[derive(Debug, Error)]
enum AppError {
    #[error("missing path after --config")]
    MissingConfigPath,
    #[error("unknown argument {0:?}; use --help for usage")]
    UnknownArgument(OsString),
    #[error("native service mode is not implemented; pass --console")]
    ConsoleModeRequired,
    #[error(transparent)]
    Persistence(#[from] PersistenceError),
    #[error(transparent)]
    Engine(#[from] EngineError),
    #[error(transparent)]
    Service(#[from] ServiceError),
}
