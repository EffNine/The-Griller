//! Command handlers for the Griller CLI.

use std::path::Path;

use griller_core::command::{CommandSpec, EnvironmentPolicy, NetworkPolicy};
use griller_core::evidence::Evidence;
use griller_policy::policy::Policy;
use griller_runtime::collectors::{
    CollectorEnvironment, CommandExecutionCollector, FileContentCollector, GitDiffCollector,
    ManifestCollector, RepositoryStateCollector, SnapshotCollector,
};
use griller_runtime::error::RuntimeError;
use griller_runtime::execution::{default_env_allowlist, EnvSource};
use griller_runtime::store::{EvidenceStore, JsonStore};
use griller_runtime::worktree::Worktree;
use serde_json::json;

use crate::output::to_json;

/// Exit code for a successful run.
const EXIT_OK: i32 = 0;
/// Exit code for a degraded/error run.
const EXIT_ERR: i32 = 1;
/// Exit code for a system/execution error (never conflated with BLOCK).
const EXIT_ERROR: i32 = 3;

fn print_value(value: &serde_json::Value, pretty: bool) -> i32 {
    println!("{}", to_json(value, pretty));
    EXIT_OK
}

fn print_error(err: &RuntimeError) -> i32 {
    eprintln!("error: {err}");
    EXIT_ERR
}

fn save_evidence(store_dir: Option<&Path>, evidence: &[Evidence]) -> Result<(), RuntimeError> {
    if let Some(dir) = store_dir {
        let mut store = JsonStore::new(dir)?;
        for item in evidence {
            store.save(item)?;
        }
    }
    Ok(())
}

fn evidence_to_value(evidence: &Evidence) -> serde_json::Value {
    serde_json::to_value(evidence).expect("evidence serializes to JSON")
}

/// `griller health`
pub fn health() -> i32 {
    let env = EnvSource::from_host();
    let git = griller_runtime::git::git_version(&env);
    let status = if git.is_some() { "ok" } else { "degraded" };
    let value = json!({
        "status": status,
        "griller_version": env!("CARGO_PKG_VERSION"),
        "git": git,
        "note": "no LLM involved; all evidence is collected deterministically",
    });
    print_value(&value, false);
    if git.is_some() {
        EXIT_OK
    } else {
        EXIT_ERR
    }
}

/// `griller context --repo <path>`
pub fn context(repo: &Path, pretty: bool) -> i32 {
    let collector = RepositoryStateCollector::new(CollectorEnvironment::host());
    match collector.collect_context(repo) {
        Ok(collected) => {
            let value = serde_json::to_value(&collected.context)
                .expect("project context serializes to JSON");
            print_value(&value, pretty)
        }
        Err(err) => print_error(&err),
    }
}

/// `griller context check --task <text> --repo <path> [--diva] [--pretty]`
///
/// M4 context adjudication. The check is **read-only**: it collects a
/// deterministic project identity, normalizes the task, and produces a
/// context assessment with an operational verdict via the M3 policy bridge.
///
/// Exit code: 0 = PASS, 1 = WARN, 2 = BLOCK, 3 = system/execution error.
pub fn context_check(task: &str, repo: &Path, diva: bool, pretty: bool) -> i32 {
    let collector =
        griller_runtime::identity::ProjectIdentityCollector::new(CollectorEnvironment::host());
    let project = match collector.collect(repo) {
        Ok(project) => project,
        Err(err) => return print_error(&err),
    };

    let assessor = griller_context::ContextAssessor::deterministic();
    let assessment = match assessor.assess(task, &project) {
        Ok(assessment) => assessment,
        Err(err) => {
            eprintln!("error: {err}");
            return EXIT_ERROR;
        }
    };

    let finding = griller_policy::ContextFinding::from_assessment(assessment);
    let exit = finding.verdict.exit_code();

    if diva {
        let renderer = griller_policy::DivaRenderer;
        eprintln!("{}", renderer.render_context_finding(&finding));
    }

    let value = json!({
        "task_identity": finding.assessment.task,
        "project_identity": finding.assessment.project,
        "context_status": finding.status().code(),
        "positive_evidence": finding.assessment.positive_evidence,
        "negative_evidence": finding.assessment.negative_evidence,
        "severity": finding.severity,
        "operational_decision": finding.verdict,
        "reason_code": finding.reason_code,
        "required_action": finding.required_action,
        "exit_code": exit,
        "note": "context assessment; the operational verdict is not a truth claim",
    });
    print_value(&value, pretty);
    exit
}

fn resolve_dir(repo: &Path) -> Result<std::path::PathBuf, RuntimeError> {
    repo.canonicalize().map_err(RuntimeError::Io)
}

fn collect_evidence(
    collector: &dyn griller_runtime::collectors::Collector,
    repo: &Path,
    store_dir: Option<&Path>,
    pretty: bool,
) -> i32 {
    match collector.collect(repo) {
        Ok(evidence) => {
            if let Err(err) = save_evidence(store_dir, &evidence) {
                return print_error(&err);
            }
            let values: Vec<serde_json::Value> = evidence.iter().map(evidence_to_value).collect();
            print_value(&json!({ "evidence": values }), pretty)
        }
        Err(err) => print_error(&err),
    }
}

/// `griller evidence git-diff --repo <path>`
pub fn evidence_git_diff(repo: &Path, store: Option<&Path>, pretty: bool) -> i32 {
    let collector = GitDiffCollector::new(EnvSource::from_host());
    collect_evidence(&collector, repo, store, pretty)
}

/// `griller evidence manifest --repo <path>`
pub fn evidence_manifest(repo: &Path, store: Option<&Path>, pretty: bool) -> i32 {
    let collector = ManifestCollector;
    collect_evidence(&collector, repo, store, pretty)
}

/// `griller evidence snapshot --repo <path>`
pub fn evidence_snapshot(repo: &Path, store: Option<&Path>, pretty: bool) -> i32 {
    let collector = SnapshotCollector::new(EnvSource::from_host());
    collect_evidence(&collector, repo, store, pretty)
}

/// `griller evidence file --repo <path> --path <file>`
pub fn evidence_file(repo: &Path, path: &Path, store: Option<&Path>, pretty: bool) -> i32 {
    let collector = FileContentCollector {
        relative_path: path.to_path_buf(),
    };
    collect_evidence(&collector, repo, store, pretty)
}

fn command_spec_from(command: &[String]) -> Result<CommandSpec, RuntimeError> {
    let (executable, args) = match command.split_first() {
        Some((exec, args)) => (exec, args),
        None => return Err(RuntimeError::Execution("no command given".to_string())),
    };
    Ok(CommandSpec::new(executable)
        .with_args(args.iter().cloned())
        .with_environment_policy(EnvironmentPolicy::InheritAllowlist(default_env_allowlist()))
        .with_network_policy(NetworkPolicy::Disabled))
}

/// Build the directory to run in: an isolated worktree unless `in_place`.
///
/// Returns the directory and an optional guard that keeps the worktree alive.
fn execution_dir(
    repo: &Path,
    in_place: bool,
) -> Result<(std::path::PathBuf, Option<Worktree>), RuntimeError> {
    if in_place {
        return Ok((resolve_dir(repo)?, None));
    }
    match Worktree::create(repo) {
        Ok(worktree) => Ok((worktree.path().to_path_buf(), Some(worktree))),
        Err(RuntimeError::Git(_)) => Ok((resolve_dir(repo)?, None)),
        Err(err) => Err(err),
    }
}

/// `griller evidence command --repo <path> -- <cmd> [args...]`
pub fn evidence_command(
    repo: &Path,
    command: &[String],
    in_place: bool,
    store: Option<&Path>,
    pretty: bool,
) -> i32 {
    let spec = match command_spec_from(command) {
        Ok(spec) => spec,
        Err(err) => return print_error(&err),
    };
    let (dir, worktree) = match execution_dir(repo, in_place) {
        Ok(pair) => pair,
        Err(err) => return print_error(&err),
    };

    let collector = CommandExecutionCollector::new(EnvSource::from_host());
    let evidence = match collector.collect(&spec, &dir) {
        Ok(evidence) => evidence,
        Err(err) => return print_error(&err),
    };

    if let Err(err) = save_evidence(store, std::slice::from_ref(&evidence)) {
        return print_error(&err);
    }
    drop(worktree);

    print_value(&json!({ "evidence": evidence_to_value(&evidence) }), pretty)
}

/// `griller run --repo <path> -- <cmd> [args...]`
pub fn run(
    repo: &Path,
    command: &[String],
    in_place: bool,
    store: Option<&Path>,
    pretty: bool,
) -> i32 {
    let spec = match command_spec_from(command) {
        Ok(spec) => spec,
        Err(err) => return print_error(&err),
    };
    let (dir, worktree) = match execution_dir(repo, in_place) {
        Ok(pair) => pair,
        Err(err) => return print_error(&err),
    };

    let collector = CommandExecutionCollector::new(EnvSource::from_host());
    let evidence = match collector.collect(&spec, &dir) {
        Ok(evidence) => evidence,
        Err(err) => return print_error(&err),
    };

    if let Err(err) = save_evidence(store, std::slice::from_ref(&evidence)) {
        return print_error(&err);
    }

    let isolated = worktree.is_some();
    let outcome = evidence
        .content
        .as_command_output()
        .map(|c| c.outcome())
        .map(|o| format!("{o:?}"));
    drop(worktree);

    print_value(
        &json!({
            "command": spec.display_line(),
            "run_in": dir,
            "isolated": isolated,
            "outcome": outcome,
            "evidence": evidence_to_value(&evidence),
        }),
        pretty,
    )
}

// ---------------------------------------------------------------------------
// M2 claim commands
// ---------------------------------------------------------------------------

/// Parse a claim source flag.
fn parse_claim_source(source: &str) -> Result<griller_adjudicate::ClaimSource, String> {
    match source.trim().to_lowercase().as_str() {
        "agent" => Ok(griller_adjudicate::ClaimSource::Agent),
        "user" => Ok(griller_adjudicate::ClaimSource::User),
        "derived" => Ok(griller_adjudicate::ClaimSource::Derived),
        "system_observed" | "system" | "observed" => {
            Ok(griller_adjudicate::ClaimSource::SystemObserved)
        }
        other => Err(format!(
            "unknown claim source {other:?}; expected agent | user | derived | system_observed"
        )),
    }
}

/// `griller claim normalize --claim <text> [--source <s>] [--pretty]`
pub fn claim_normalize(claim: &str, source: &str, pretty: bool) -> i32 {
    let source = match parse_claim_source(source) {
        Ok(s) => s,
        Err(err) => {
            eprintln!("error: {err}");
            return EXIT_ERR;
        }
    };
    let input = griller_adjudicate::ClaimInput::new(claim, source);
    let adjudicator = griller_adjudicate::Adjudicator::deterministic();
    match adjudicator.normalize(&input) {
        Ok(normalized) => {
            let value = serde_json::to_value(normalized).expect("normalized claim serializes");
            print_value(&value, pretty)
        }
        Err(err) => {
            eprintln!("error: {err}");
            EXIT_ERR
        }
    }
}

/// `griller claim assess --claim <text> --repo <path> [--evidence-dir <dir>]
/// [--auto-collect] [--pretty]`
pub fn claim_assess(
    claim: &str,
    source: &str,
    repo: &Path,
    evidence_dir: Option<&Path>,
    auto_collect: bool,
    pretty: bool,
) -> i32 {
    let source = match parse_claim_source(source) {
        Ok(s) => s,
        Err(err) => {
            eprintln!("error: {err}");
            return EXIT_ERR;
        }
    };
    let input = griller_adjudicate::ClaimInput::new(claim, source);
    let adjudicator = griller_adjudicate::Adjudicator::deterministic();

    let normalized = match adjudicator.normalize(&input) {
        Ok(nc) => nc,
        Err(err) => {
            eprintln!("error: {err}");
            return EXIT_ERR;
        }
    };

    let evidence = match collect_claim_evidence(&normalized, repo, evidence_dir, auto_collect) {
        Ok(evidence) => evidence,
        Err(err) => return print_error(&err),
    };

    match adjudicator.assess(&input, &evidence, None) {
        Ok(assessment) => {
            let value = serde_json::to_value(&assessment).expect("assessment serializes to JSON");
            let value = json!({
                "assessment": value,
                "evidence_used": evidence.len(),
                "note": "epistemic status only; no operational verdict is produced in M2",
            });
            print_value(&value, pretty)
        }
        Err(err) => {
            eprintln!("error: {err}");
            EXIT_ERR
        }
    }
}

/// Collect evidence for a claim from an evidence store, auto-collection, or
/// nothing (in that order of precedence).
fn collect_claim_evidence(
    normalized: &griller_adjudicate::NormalizedClaim,
    repo: &Path,
    evidence_dir: Option<&Path>,
    auto_collect: bool,
) -> Result<Vec<griller_core::evidence::Evidence>, griller_runtime::error::RuntimeError> {
    if let Some(dir) = evidence_dir {
        let store = griller_runtime::store::JsonStore::new(dir)?;
        return store.load_all();
    }
    if auto_collect {
        return collect_for_claim(normalized, repo);
    }
    Ok(Vec::new())
}

/// Parse a severity flag.
fn parse_severity(value: &str) -> Result<griller_policy::severity::Severity, String> {
    match value.trim().to_lowercase().as_str() {
        "info" => Ok(griller_policy::severity::Severity::Info),
        "low" => Ok(griller_policy::severity::Severity::Low),
        "medium" => Ok(griller_policy::severity::Severity::Medium),
        "high" => Ok(griller_policy::severity::Severity::High),
        "critical" => Ok(griller_policy::severity::Severity::Critical),
        other => Err(format!(
            "unknown severity {other:?}; expected info | low | medium | high | critical"
        )),
    }
}

/// `griller claim decide --claim <text>... [--severity <s>] [--repo <path>]
/// [--production] [--refactor] [--evidence-dir <dir>] [--auto-collect]
/// [--diva] [--pretty]`
///
/// Applies the M3 policy to each claim's M2 assessment and aggregates. Exit
/// code: 0 = PASS, 1 = WARN, 2 = BLOCK, 3 = system/execution error.
///
/// The flat argument list mirrors the clap CLI surface; this is a thin wiring
/// function, so the extra parameters are acceptable.
#[allow(clippy::too_many_arguments)]
pub fn claim_decide(
    claims: &[String],
    source: &str,
    repo: &Path,
    severity: Option<&str>,
    production: bool,
    refactor: bool,
    evidence_dir: Option<&Path>,
    auto_collect: bool,
    diva: bool,
    pretty: bool,
) -> i32 {
    let source = match parse_claim_source(source) {
        Ok(s) => s,
        Err(err) => {
            eprintln!("error: {err}");
            return EXIT_ERROR;
        }
    };
    let declared = match severity {
        Some(value) => match parse_severity(value) {
            Ok(s) => Some(s),
            Err(err) => {
                eprintln!("error: {err}");
                return EXIT_ERROR;
            }
        },
        None => None,
    };

    let adjudicator = griller_adjudicate::Adjudicator::deterministic();
    let resolver = griller_policy::severity::SeverityResolver;
    let signals = griller_policy::severity::SeveritySignals {
        declared,
        touches_production: production,
        simple_refactor: refactor,
    };

    let mut inputs: Vec<griller_policy::PolicyInput> = Vec::new();
    for claim in claims {
        let input = griller_adjudicate::ClaimInput::new(claim, source);
        let normalized = match adjudicator.normalize(&input) {
            Ok(nc) => nc,
            Err(err) => {
                eprintln!("error: {err}");
                return EXIT_ERROR;
            }
        };
        let evidence = match collect_claim_evidence(&normalized, repo, evidence_dir, auto_collect) {
            Ok(evidence) => evidence,
            Err(err) => {
                eprintln!("error: {err}");
                return EXIT_ERROR;
            }
        };
        let assessment = match adjudicator.assess(&input, &evidence, None) {
            Ok(a) => a,
            Err(err) => {
                eprintln!("error: {err}");
                return EXIT_ERROR;
            }
        };
        let resolved = resolver.resolve(assessment.claim.kind, &signals);
        inputs.push(griller_policy::PolicyInput {
            assessment,
            context: griller_policy::PolicyContext::new(resolved.severity, resolved.source),
        });
    }

    let overall = griller_policy::PolicyTable.decide_many(&inputs);
    let exit = overall.verdict.exit_code();

    if diva {
        let renderer = griller_policy::DivaRenderer;
        eprintln!("{}", renderer.render_overall(&overall));
    }

    let value = serde_json::to_value(&overall).expect("overall decision serializes to JSON");
    let value = json!({
        "decision": value,
        "exit_code": exit,
        "note": "operational verdict; not a truth claim",
    });
    print_value(&value, pretty);
    exit
}

/// Detect a build command from repository manifests.
fn detect_build_command(repo: &Path) -> Option<(&'static str, Vec<&'static str>)> {
    if repo.join("Cargo.toml").exists() {
        Some(("cargo", vec!["build"]))
    } else if repo.join("package.json").exists() {
        Some(("npm", vec!["run", "build"]))
    } else if repo.join("go.mod").exists() {
        Some(("go", vec!["build", "./..."]))
    } else if repo.join("Makefile").exists() {
        Some(("make", vec![]))
    } else {
        None
    }
}

/// Detect a test command from repository manifests.
fn detect_test_command(repo: &Path) -> Option<(&'static str, Vec<&'static str>)> {
    if repo.join("Cargo.toml").exists() {
        Some(("cargo", vec!["test"]))
    } else if repo.join("package.json").exists() {
        Some(("npm", vec!["test"]))
    } else if repo.join("go.mod").exists() {
        Some(("go", vec!["test", "./..."]))
    } else if repo.join("pyproject.toml").exists() {
        Some(("pytest", vec![]))
    } else {
        None
    }
}

/// Detect a formatting command from repository manifests.
fn detect_format_command(repo: &Path) -> Option<(&'static str, Vec<&'static str>)> {
    if repo.join("Cargo.toml").exists() {
        Some(("cargo", vec!["fmt", "--check"]))
    } else if repo.join("package.json").exists() {
        Some(("npx", vec!["prettier", "--check", "."]))
    } else if repo.join("pyproject.toml").exists() {
        Some(("black", vec!["--check", "."]))
    } else {
        None
    }
}

/// Collect evidence for a normalized claim by running matching commands in an
/// isolated worktree (never the user's tree).
fn collect_for_claim(
    normalized: &griller_adjudicate::NormalizedClaim,
    repo: &Path,
) -> Result<Vec<griller_core::evidence::Evidence>, griller_runtime::error::RuntimeError> {
    use griller_adjudicate::claim_class;
    use griller_runtime::collectors::{Collector, GitDiffCollector, SnapshotCollector};

    let class = claim_class(normalized);
    let env = EnvSource::from_host();
    let (dir, worktree) = execution_dir(repo, false)?;

    let mut evidence: Vec<griller_core::evidence::Evidence> = Vec::new();
    let command: Option<(&'static str, Vec<&'static str>)> = match class {
        griller_adjudicate::ClaimClass::TestsPass | griller_adjudicate::ClaimClass::TestsFail => {
            detect_test_command(&dir)
        }
        griller_adjudicate::ClaimClass::BuildSucceeds
        | griller_adjudicate::ClaimClass::BuildFails => detect_build_command(&dir),
        griller_adjudicate::ClaimClass::FormattingPasses
        | griller_adjudicate::ClaimClass::FormattingFails => detect_format_command(&dir),
        griller_adjudicate::ClaimClass::FileChanged
        | griller_adjudicate::ClaimClass::FileRemoved
        | griller_adjudicate::ClaimClass::EntityExists
        | griller_adjudicate::ClaimClass::EntityAbsent => None,
        griller_adjudicate::ClaimClass::Unknown => None,
    };

    if let Some((exe, args)) = command {
        let spec = CommandSpec::new(exe)
            .with_args(args.iter().map(|s| s.to_string()))
            .with_environment_policy(EnvironmentPolicy::InheritAllowlist(default_env_allowlist()))
            .with_network_policy(NetworkPolicy::Disabled);
        let collector = CommandExecutionCollector::new(env.clone());
        let ev = collector.collect(&spec, &dir)?;
        evidence.push(ev);
    }

    if matches!(
        class,
        griller_adjudicate::ClaimClass::FileChanged
            | griller_adjudicate::ClaimClass::FileRemoved
            | griller_adjudicate::ClaimClass::EntityExists
            | griller_adjudicate::ClaimClass::EntityAbsent
    ) {
        if let Ok(diff) = GitDiffCollector::new(env.clone()).collect(&dir) {
            evidence.extend(diff);
        }
        if let Ok(snap) = SnapshotCollector::new(env.clone()).collect(&dir) {
            evidence.extend(snap);
        }
    }

    drop(worktree);
    Ok(evidence)
}

/// `griller attack run --repo <path> --claim "Tests pass" [--pretty] [--auto-collect]`
///
/// M5 adversarial attack: runs applicable attacks against a claim and produces
/// structured evidence. Attacks are deterministic and replayable.
///
/// Exit code: 0 = success, 1 = error.
/// `griller attack run --repo <path> --claim "Tests pass" [--pretty] [--auto-collect]`
///
/// M5 adversarial attack: runs applicable attacks against a claim and produces
/// structured evidence. Attacks are deterministic and replayable.
///
/// Currently implements Attack #1 — REPLAY: replays the claimed verification
/// command (cargo test) and reports the outcome.
///
/// Exit code: 0 = success, 1 = error.
pub fn attack_run(repo: &Path, claim: &str, pretty: bool, auto_collect: bool) -> i32 {
    let env = EnvSource::from_host();

    // Normalize the claim to extract predicates and target
    let input = griller_adjudicate::ClaimInput::new(claim, griller_adjudicate::ClaimSource::Agent);
    let adjudicator = griller_adjudicate::Adjudicator::deterministic();
    let normalized = match adjudicator.normalize(&input) {
        Ok(nc) => nc,
        Err(err) => {
            eprintln!("error: {err}");
            return EXIT_ERR;
        }
    };

    // Collect evidence if auto_collect is enabled
    let evidence = if auto_collect {
        match collect_claim_evidence(&normalized, repo, None, auto_collect) {
            Ok(ev) => ev,
            Err(err) => return print_error(&err),
        }
    } else {
        Vec::new()
    };

    // Assess the claim using M2 adjudication
    let assessment = match adjudicator.assess(&input, &evidence, None) {
        Ok(a) => a,
        Err(err) => {
            eprintln!("error: {err}");
            return EXIT_ERR;
        }
    };

    // Run M5 Attack #1 — REPLAY: replay the claimed verification command.
    // Replay `cargo test` in an isolated worktree (or in-place if worktree
    // unavailable). This is the deterministic, replay-based adversarial attack.
    let dir = if std::env::var("GRILLER_IN_PLACE").is_ok_and(|v| v == "1") {
        // In-place mode
        resolve_dir(repo).unwrap_or_else(|_| {
            repo.to_path_buf()
                .canonicalize()
                .unwrap_or(repo.to_path_buf())
        })
    } else {
        // Try worktree isolation
        match Worktree::create(repo) {
            Ok(worktree) => worktree.path().to_path_buf(),
            Err(RuntimeError::Git(_)) => {
                // Fall back to in-place
                resolve_dir(repo).unwrap_or_else(|_| {
                    repo.to_path_buf()
                        .canonicalize()
                        .unwrap_or(repo.to_path_buf())
                })
            }
            Err(err) => {
                eprintln!("error creating worktree: {err}");
                return EXIT_ERR;
            }
        }
    };

    // Execute cargo test in the working directory
    let spec = CommandSpec::new("cargo").with_args(["test"]);

    let collector = CommandExecutionCollector::new(env.clone());
    match collector.collect(&spec, &dir) {
        Ok(evidence) => {
            let outcome = evidence
                .content
                .as_command_output()
                .map(|c| c.outcome())
                .map(|o| format!("{o:?}"));
            let value = json!({
                "claim": claim,
                "attack": "replay",
                "outcome": outcome,
                "status": assessment.status,
            });
            print_value(&value, pretty)
        }
        Err(err) => {
            eprintln!("error: {err}");
            // Still report the M2 assessment even if replay fails
            let value = json!({
                "claim": claim,
                "attack": "replay",
                "outcome": "execution_failed",
                "status": assessment.status,
                "error": format!("{err}"),
            });
            print_value(&value, pretty)
        }
    }
}

/// `griller self-grill --claim <text> --repo <path> [--evidence-dir <dir>]
/// [--auto-collect] [--diva] [--pretty]`
///
/// M7 self-grill: audit the Griller's own decision pipeline.
/// Produces an immutable original decision + a self-grill result.
/// Does NOT overwrite the original verdict.
pub fn self_grill(
    claim: &str,
    source: &str,
    repo: &Path,
    evidence_dir: Option<&Path>,
    auto_collect: bool,
    diva: bool,
    pretty: bool,
) -> i32 {
    let source = match parse_claim_source(source) {
        Ok(s) => s,
        Err(err) => {
            eprintln!("error: {err}");
            return EXIT_ERROR;
        }
    };

    let input = griller_adjudicate::ClaimInput::new(claim, source);
    let adjudicator = griller_adjudicate::Adjudicator::deterministic();

    let normalized = match adjudicator.normalize(&input) {
        Ok(nc) => nc,
        Err(err) => {
            eprintln!("error: {err}");
            return EXIT_ERR;
        }
    };

    let evidence = match collect_claim_evidence(&normalized, repo, evidence_dir, auto_collect) {
        Ok(evidence) => evidence,
        Err(err) => return print_error(&err),
    };

    let assessment = match adjudicator.assess(&input, &evidence, None) {
        Ok(a) => a,
        Err(err) => {
            eprintln!("error: {err}");
            return EXIT_ERROR;
        }
    };

    let resolver = griller_policy::severity::SeverityResolver;
    let signals = griller_policy::severity::SeveritySignals {
        declared: None,
        touches_production: false,
        simple_refactor: false,
    };
    let resolved = resolver.resolve(assessment.claim.kind, &signals);

    let policy_input = griller_policy::PolicyInput {
        assessment: assessment.clone(),
        context: griller_policy::PolicyContext::new(resolved.severity, resolved.source),
    };
    let decision = griller_policy::PolicyTable.decide(&assessment, &policy_input.context);

    // Run self-grill
    let sg_input = griller_self_grill::SelfGrillInput::from_pipeline(
        &assessment,
        &decision,
        None, // context not re-checked here
        None, // attack coverage not available here
        &evidence,
        resolved.severity,
        resolved.source,
    );

    let (_original, result) = griller_self_grill::self_grill(&sg_input);

    if diva {
        eprintln!("{}", render_diva_result(&result));
    }

    let value = json!({
        "original": {
            "claim_text": claim,
            "epistemic_status": assessment.status,
            "verdict": decision.verdict,
            "reason_code": decision.reason_code,
            "required_action": format!("{:?}", decision.required_action),
            "severity": format!("{:?}", resolved.severity),
        },
        "self_grill": result,
        "note": "self-grill audits the decision pipeline; it does not modify the original verdict",
    });
    print_value(&value, pretty)
}

fn render_diva_result(result: &griller_self_grill::SelfGrillResult) -> String {
    match result.outcome {
        griller_self_grill::SelfGrillOutcome::Holds => {
            "I checked my own receipts. Annoyingly, they hold up.".to_string()
        }
        griller_self_grill::SelfGrillOutcome::Weakened => {
            "I grilled myself. Some of my own arguments are thinner than I'd like,".to_string()
        }
        griller_self_grill::SelfGrillOutcome::Refuted => {
            "I grilled myself. Unfortunately, I found a hole in my own argument.".to_string()
        }
        griller_self_grill::SelfGrillOutcome::Inconclusive => {
            "I tried to grill myself, but the evidence was too muddied to tell.".to_string()
        }
        griller_self_grill::SelfGrillOutcome::NotApplicable => {
            "There wasn't enough here to grill. Nothing to contradict, nothing to confirm."
                .to_string()
        }
    }
}
