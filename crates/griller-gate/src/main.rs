//! `griller-gate` — the automatic completion gate CLI.
//!
//! One-shot per completion event (no daemon, no polling):
//!
//! ```text
//! griller-gate check --config <gate-config.json>
//!   stdin  : CompletionEvent JSON
//!   stdout : GateDecision JSON
//!   stderr : lifecycle log lines
//! ```
//!
//! Exit codes: `0` a decision was produced (including `unavailable`),
//! `2` configuration error, `3` the gate could not produce a decision.

#![forbid(unsafe_code)]

use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use griller_gate::config::GateConfig;
use griller_gate::runner::{run_gate, GateOutcome};
use griller_gate::{CompletionEvent, CompletionExtraction, GATE_SCHEMA_VERSION};

const EXIT_OK: u8 = 0;
const EXIT_CONFIG_ERROR: u8 = 2;
const EXIT_NO_DECISION: u8 = 3;

#[derive(Parser)]
#[command(
    name = "griller-gate",
    version,
    about = "Automatic completion gate for The Griller"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Verify one completion event against the existing griller-mcp pipeline.
    Check {
        /// Path to the gate configuration JSON.
        #[arg(long)]
        config: PathBuf,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Check { config } => run_check(&config),
    }
}

fn run_check(config_path: &PathBuf) -> ExitCode {
    let config: GateConfig = match load_config(config_path) {
        Ok(config) => config,
        Err(message) => {
            eprintln!("gate error: {message}");
            return ExitCode::from(EXIT_CONFIG_ERROR);
        }
    };

    let event = match read_event() {
        Ok(event) => event,
        Err(message) => {
            eprintln!("gate error: {message}");
            return ExitCode::from(EXIT_NO_DECISION);
        }
    };

    // The gate itself never decides when a completion boundary occurs; the
    // host does. The HEAD commit is resolved here (shell-free) so the event
    // records what the work was against.
    let event = griller_gate::runner::with_head_commit(event);

    eprintln!(
        "griller-gate: COMPLETION_EVENT repo={} session={}",
        event.repository,
        event.session_id.as_deref().unwrap_or("-")
    );
    let outcome = run_gate(&event, &config);
    match outcome {
        GateOutcome::Decision(ref decision) => {
            let line = decision.summary_line();
            eprintln!("griller-gate: {line} ({:?}ms)", decision.timing.total_ms);
            eprintln!("griller-gate: lifecycle: {}", render_lifecycle(decision));
            match serde_json::to_string_pretty(&decision) {
                Ok(json) => println!("{json}"),
                Err(e) => {
                    eprintln!("gate error: decision serialization failed: {e}");
                    return ExitCode::from(EXIT_NO_DECISION);
                }
            }
            if let Err(e) = persist_decision(&config, decision) {
                // Persistence failure is diagnostic only: the decision is
                // still returned to the host.
                eprintln!("griller-gate: warning: could not persist decision: {e}");
            }
            ExitCode::from(EXIT_OK)
        }
        GateOutcome::Failed(e) => {
            eprintln!("gate error: {e}");
            ExitCode::from(EXIT_NO_DECISION)
        }
    }
}

fn load_config(path: &PathBuf) -> Result<GateConfig, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read gate config {:?}: {e}", path.display()))?;
    let config: GateConfig = serde_json::from_str(&text)
        .map_err(|e| format!("gate config {:?} is invalid JSON: {e}", path.display()))?;
    config
        .validate()
        .map_err(|e| format!("gate config {:?}: {e}", path.display()))?;
    Ok(config)
}

fn read_event() -> Result<CompletionEvent, String> {
    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .map_err(|e| format!("cannot read completion event from stdin: {e}"))?;
    let event: CompletionEvent = serde_json::from_str(&input)
        .map_err(|e| format!("completion event on stdin is invalid JSON: {e}"))?;
    if event.schema_version != GATE_SCHEMA_VERSION {
        return Err(format!(
            "completion event schema mismatch: got '{}', expected '{GATE_SCHEMA_VERSION}'",
            event.schema_version
        ));
    }
    if event.repository.trim().is_empty() {
        return Err("completion event: repository must not be empty".into());
    }
    if event.claim.trim().is_empty() {
        // An empty claim is only legitimate when the host explicitly reported
        // that it found no completion assertion. Overloading an empty claim
        // without that signal would be ambiguous, which is the whole reason the
        // `extraction` field exists rather than reusing `claim`.
        if event.extraction != Some(CompletionExtraction::NoAssertion) {
            return Err("completion event: claim must not be empty".into());
        }
    }
    Ok(event)
}

fn render_lifecycle(decision: &griller_gate::GateDecision) -> String {
    decision
        .lifecycle
        .iter()
        .map(|r| r.stage.label().to_string())
        .collect::<Vec<_>>()
        .join(" -> ")
}

/// Persist the decision document and an append-only gate log under the
/// state directory. Never raises.
fn persist_decision(
    config: &GateConfig,
    decision: &griller_gate::GateDecision,
) -> Result<(), String> {
    std::fs::create_dir_all(&config.state_dir)
        .map_err(|e| format!("cannot create state dir: {e}"))?;
    let log_path = config.state_dir.join("gate.log");
    let mut log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|e| format!("cannot open gate.log: {e}"))?;
    use std::io::Write;
    writeln!(
        log,
        "{} status={} session={} repo={}",
        chrono_label(),
        decision.status,
        decision.event.session_id.as_deref().unwrap_or("-"),
        decision.event.repository
    )
    .map_err(|e| format!("cannot write gate.log: {e}"))?;

    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let decision_dir = config.state_dir.join("decisions");
    std::fs::create_dir_all(&decision_dir)
        .map_err(|e| format!("cannot create decisions dir: {e}"))?;
    let file = decision_dir.join(format!(
        "{stamp}-{}.json",
        decision.event.session_id.as_deref().unwrap_or("session")
    ));
    let json = serde_json::to_string_pretty(decision)
        .map_err(|e| format!("decision serialization failed: {e}"))?;
    std::fs::write(&file, json).map_err(|e| format!("cannot write decision file: {e}"))
}

fn chrono_label() -> String {
    // Deterministic-ish label without extra dependencies: UNIX ms.
    griller_gate::lifecycle::LifecycleLog::now_ms().to_string()
}
