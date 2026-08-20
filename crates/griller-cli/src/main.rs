//! The Griller CLI — M0/M1 foundation demonstration.
//!
//! Commands produce structured JSON as the canonical machine-readable output.

mod commands;
mod output;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "griller",
    version,
    about = "Deterministic evidence collection for verifying claims about codebases.",
    long_about = None
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Check the toolchain and environment.
    Health,
    /// Collect and print the ProjectContext for a repository, or run an M4
    /// context check.
    Context {
        /// Repository path.
        #[arg(long, default_value = ".")]
        repo: PathBuf,
        /// Print pretty-printed JSON.
        #[arg(long)]
        pretty: bool,
        #[command(subcommand)]
        sub: Option<ContextCommand>,
    },
    /// Collect evidence.
    Evidence {
        #[command(subcommand)]
        sub: EvidenceCommand,
    },
    /// Execute a command inside an isolated worktree (never the user's tree).
    Run {
        /// Repository path.
        #[arg(long, default_value = ".")]
        repo: PathBuf,
        /// Run in the repository directory instead of an isolated worktree.
        #[arg(long)]
        in_place: bool,
        /// Optional evidence store directory.
        #[arg(long)]
        store: Option<PathBuf>,
        /// Print pretty-printed JSON.
        #[arg(long)]
        pretty: bool,
        /// The command to execute (everything after `--`).
        #[arg(last = true, required = true)]
        command: Vec<String>,
    },
    /// M5 adversarial attack: run attacks against a claim to verify it.
    Attack {
        /// Repository path.
        #[arg(long, default_value = ".")]
        repo: PathBuf,
        /// The claim to attack.
        #[arg(long)]
        claim: String,
        /// Print pretty-printed JSON.
        #[arg(long)]
        pretty: bool,
        /// Collect evidence from the repository (runs commands in an
        /// isolated worktree; opt-in).
        #[arg(long)]
        auto_collect: bool,
    },
    /// M7 self-grill: audit the Griller's own decision pipeline.
    SelfGrill {
        /// The claim text.
        #[arg(long)]
        claim: String,
        /// Repository path.
        #[arg(long, default_value = ".")]
        repo: PathBuf,
        /// Claim source: agent | user | derived | system_observed.
        #[arg(long, default_value = "user")]
        source: String,
        /// Directory of a JSON evidence store to load evidence from.
        #[arg(long)]
        evidence_dir: Option<PathBuf>,
        /// Collect matching evidence from the repository (opt-in).
        #[arg(long)]
        auto_collect: bool,
        /// Emit Diva persona text to stderr (decorative; JSON stays on stdout).
        #[arg(long)]
        diva: bool,
        /// Print pretty-printed JSON.
        #[arg(long)]
        pretty: bool,
    },
    /// M2 claim adjudication: normalize a claim and decide its epistemic
    /// status from evidence. Produces an epistemic status, never an
    /// operational verdict.
    Claim {
        #[command(subcommand)]
        sub: ClaimCommand,
    },
}

/// Context-related commands (M0/M1 describe + M4 context check).
#[derive(Subcommand, Debug)]
enum ContextCommand {
    /// M4: assess whether a task belongs to this repository (read-only).
    Check {
        /// The incoming task text.
        #[arg(long)]
        task: String,
        /// Repository path.
        #[arg(long, default_value = ".")]
        repo: PathBuf,
        /// Print pretty-printed JSON.
        #[arg(long)]
        pretty: bool,
        /// Emit Diva persona text to stderr (decorative; JSON stays on stdout).
        #[arg(long)]
        diva: bool,
    },
}

/// Claim-related commands (M2).
#[derive(Subcommand, Debug)]
enum ClaimCommand {
    /// Normalize a natural-language claim into structured predicates.
    Normalize {
        /// The claim text.
        #[arg(long)]
        claim: String,
        /// Claim source: agent | user | derived | system_observed.
        #[arg(long, default_value = "user")]
        source: String,
        /// Print pretty-printed JSON.
        #[arg(long)]
        pretty: bool,
    },
    /// Assess a claim against evidence and assign an epistemic status.
    Assess {
        /// The claim text.
        #[arg(long)]
        claim: String,
        /// Repository path.
        #[arg(long, default_value = ".")]
        repo: PathBuf,
        /// Claim source: agent | user | derived | system_observed.
        #[arg(long, default_value = "user")]
        source: String,
        /// Directory of a JSON evidence store to load evidence from.
        #[arg(long)]
        evidence_dir: Option<PathBuf>,
        /// Collect matching evidence from the repository (runs commands in an
        /// isolated worktree; opt-in).
        #[arg(long)]
        auto_collect: bool,
        /// Print pretty-printed JSON.
        #[arg(long)]
        pretty: bool,
    },
    /// Decide the operational verdict (PASS/WARN/BLOCK) for one or more
    /// claims. Applies the M3 policy to each M2 assessment and aggregates.
    Decide {
        /// Claim text (repeatable for multiple findings).
        #[arg(long, required = true)]
        claim: Vec<String>,
        /// Repository path.
        #[arg(long, default_value = ".")]
        repo: PathBuf,
        /// Claim source: agent | user | derived | system_observed.
        #[arg(long, default_value = "user")]
        source: String,
        /// Explicit severity: info | low | medium | high | critical.
        #[arg(long)]
        severity: Option<String>,
        /// Structured signal: the claim touches production behaviour.
        #[arg(long)]
        production: bool,
        /// Structured signal: the claim is a simple/mechanical refactor.
        #[arg(long)]
        refactor: bool,
        /// Directory of a JSON evidence store to load evidence from.
        #[arg(long)]
        evidence_dir: Option<PathBuf>,
        /// Collect matching evidence from the repository (runs commands in an
        /// isolated worktree; opt-in).
        #[arg(long)]
        auto_collect: bool,
        /// Emit Diva persona text to stderr (decorative; JSON stays on stdout).
        #[arg(long)]
        diva: bool,
        /// Print pretty-printed JSON.
        #[arg(long)]
        pretty: bool,
    },
}

#[derive(Subcommand, Debug)]
enum EvidenceCommand {
    /// Collect git diff evidence.
    GitDiff {
        /// Repository path.
        #[arg(long, default_value = ".")]
        repo: PathBuf,
        /// Optional evidence store directory.
        #[arg(long)]
        store: Option<PathBuf>,
        /// Print pretty-printed JSON.
        #[arg(long)]
        pretty: bool,
    },
    /// Collect manifest evidence for every detected manifest.
    Manifest {
        /// Repository path.
        #[arg(long, default_value = ".")]
        repo: PathBuf,
        /// Optional evidence store directory.
        #[arg(long)]
        store: Option<PathBuf>,
        /// Print pretty-printed JSON.
        #[arg(long)]
        pretty: bool,
    },
    /// Collect evidence for a single file.
    File {
        /// Repository path.
        #[arg(long, default_value = ".")]
        repo: PathBuf,
        /// File path relative to the repository root.
        #[arg(long)]
        path: PathBuf,
        /// Optional evidence store directory.
        #[arg(long)]
        store: Option<PathBuf>,
        /// Print pretty-printed JSON.
        #[arg(long)]
        pretty: bool,
    },
    /// Execute a command and collect its output as evidence.
    Command {
        /// Repository path.
        #[arg(long, default_value = ".")]
        repo: PathBuf,
        /// Run in the repository directory instead of an isolated worktree.
        #[arg(long)]
        in_place: bool,
        /// Optional evidence store directory.
        #[arg(long)]
        store: Option<PathBuf>,
        /// Print pretty-printed JSON.
        #[arg(long)]
        pretty: bool,
        /// The command to execute (everything after `--`).
        #[arg(last = true, required = true)]
        command: Vec<String>,
    },
    /// Collect a read-only repository snapshot (base + working state).
    Snapshot {
        /// Repository path.
        #[arg(long, default_value = ".")]
        repo: PathBuf,
        /// Optional evidence store directory.
        #[arg(long)]
        store: Option<PathBuf>,
        /// Print pretty-printed JSON.
        #[arg(long)]
        pretty: bool,
    },
}

fn main() {
    let cli = Cli::parse();
    let exit_code = run(cli);
    std::process::exit(exit_code);
}

fn run(cli: Cli) -> i32 {
    match cli.command {
        Command::Health => commands::health(),
        Command::Context { repo, pretty, sub } => match sub {
            Some(ContextCommand::Check {
                task,
                repo: task_repo,
                pretty: task_pretty,
                diva,
            }) => commands::context_check(&task, &task_repo, diva, task_pretty),
            None => commands::context(&repo, pretty),
        },
        Command::Evidence { sub } => match sub {
            EvidenceCommand::GitDiff {
                repo,
                store,
                pretty,
            } => commands::evidence_git_diff(&repo, store.as_deref(), pretty),
            EvidenceCommand::Manifest {
                repo,
                store,
                pretty,
            } => commands::evidence_manifest(&repo, store.as_deref(), pretty),
            EvidenceCommand::File {
                repo,
                path,
                store,
                pretty,
            } => commands::evidence_file(&repo, &path, store.as_deref(), pretty),
            EvidenceCommand::Command {
                repo,
                in_place,
                store,
                pretty,
                command,
            } => commands::evidence_command(&repo, &command, in_place, store.as_deref(), pretty),
            EvidenceCommand::Snapshot {
                repo,
                store,
                pretty,
            } => commands::evidence_snapshot(&repo, store.as_deref(), pretty),
        },
        Command::Run {
            repo,
            in_place,
            store,
            pretty,
            command,
        } => commands::run(&repo, &command, in_place, store.as_deref(), pretty),
        Command::Claim { sub } => match sub {
            ClaimCommand::Normalize {
                claim,
                source,
                pretty,
            } => commands::claim_normalize(&claim, &source, pretty),
            ClaimCommand::Assess {
                claim,
                repo,
                source,
                evidence_dir,
                auto_collect,
                pretty,
            } => commands::claim_assess(
                &claim,
                &source,
                &repo,
                evidence_dir.as_deref(),
                auto_collect,
                pretty,
            ),
            ClaimCommand::Decide {
                claim,
                repo,
                source,
                severity,
                production,
                refactor,
                evidence_dir,
                auto_collect,
                diva,
                pretty,
            } => commands::claim_decide(
                &claim,
                &source,
                &repo,
                severity.as_deref(),
                production,
                refactor,
                evidence_dir.as_deref(),
                auto_collect,
                diva,
                pretty,
            ),
        },
        Command::Attack {
            repo,
            claim,
            pretty,
            auto_collect,
        } => commands::attack_run(&repo, &claim, pretty, auto_collect),
        Command::SelfGrill {
            claim,
            repo,
            source,
            evidence_dir,
            auto_collect,
            diva,
            pretty,
        } => commands::self_grill(
            &claim,
            &source,
            &repo,
            evidence_dir.as_deref(),
            auto_collect,
            diva,
            pretty,
        ),
    }
}
