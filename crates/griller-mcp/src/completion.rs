//! `check_completion` — the M8 golden path.
//!
//! This module turns the six-stage pipeline into one coherent, deterministic,
//! agent-facing workflow:
//!
//! ```text
//! validate request
//!   -> M4 project identity + task identity + context assessment
//!   -> (context mismatch => M3 BLOCK, no attacks)
//!   -> repository snapshot + evidence collection
//!   -> M2 initial adjudication
//!   -> M5 attack planning + safe execution (replay in an isolated worktree)
//!   -> M2 re-adjudication with attack evidence
//!   -> M3 policy decision
//!   -> M7 self-grill on the real pipeline data
//!   -> CompletionReport
//! ```
//!
//! The Griller judges; it does not modify the repository, fix code, or retry
//! the agent. The original decision is immutable: self-grill never overrides
//! it (see [`CompletionSelfGrillSection`]).
//!
//! Every stage is distinguishable in the report and timing is measured
//! per-stage so expensive operations can be identified without pretending to
//! be confidence scores.

use std::path::{Path, PathBuf};
use std::time::Instant;

use griller_adjudicate::assess::Adjudicator;
use griller_adjudicate::attack_coverage::AttackCoverage as M2AttackCoverage;
use griller_adjudicate::claim::{ClaimInput, ClaimKind, ClaimSource};
use griller_adjudicate::relationship::EvidenceRelationship;
use griller_adjudicate::status::{EpistemicStatus, InsufficiencyNote, Reason};
use griller_context::assess::ContextAssessor;
use griller_context::status::ContextStatus;
use griller_core::attack::{AttackKind, AttackOutcome, AttackResult, AttackSpec};
use griller_core::command::{CommandSpec, ReplayLock, REPLAY_TIMEOUT_MS};
use griller_core::evidence::{Evidence, EvidenceContent, EvidenceId};
use griller_policy::context::ContextFinding;
use griller_policy::decision::RequiredAction;
use griller_policy::policy::{Policy, PolicyContext, PolicyTable};
use griller_policy::severity::{Severity, SeveritySource};
use griller_policy::verdict::Verdict;
use griller_runtime::attack::AttackExecutor;
use griller_runtime::collectors::{
    Collector, CollectorEnvironment, CommandExecutionCollector, FileContentCollector,
    GitDiffCollector, RepositoryStateCollector, SnapshotCollector,
};
use griller_runtime::error::RuntimeError;
use griller_runtime::execution::EnvSource;
use griller_runtime::identity::ProjectIdentityCollector;
use griller_runtime::worktree::Worktree;

use crate::error::{McpError, McpErrorKind};
use crate::plan;
use crate::validate::MAX_EVIDENCE_SNIPPETS;

/// Canonical schema version for completion reports.
pub const SCHEMA_VERSION: &str = "m8.0";

/// A validated completion-check request.
#[derive(Debug, Clone)]
pub struct CompletionRequest {
    /// Canonical repository path.
    pub repo: PathBuf,
    /// The agent's completion claim (verbatim).
    pub claim: String,
    /// Claim kind (defaults to Completion at the adapter layer).
    pub claim_kind: ClaimKind,
    /// Per-attack timeout in milliseconds.
    pub timeout_ms: u64,
}

/// Run the full check_completion pipeline for one request.
pub fn run_completion_check(req: &CompletionRequest) -> Result<CompletionReport, McpError> {
    let started = Instant::now();
    let env = EnvSource::from_host();

    // -----------------------------------------------------------------------
    // M4: project identity + task identity + context assessment
    // -----------------------------------------------------------------------
    let identity_start = Instant::now();
    let identity_collector =
        ProjectIdentityCollector::new(CollectorEnvironment { env: env.clone() });
    let project = identity_collector
        .collect(&req.repo)
        .map_err(|e| McpError {
            kind: McpErrorKind::ExecutionFailed,
            message: format!("project identity collection failed: {e}"),
        })?;
    let t_identity = identity_start.elapsed().as_millis() as u64;

    let context_start = Instant::now();
    let assessor = ContextAssessor::deterministic();
    let context_assessment = assessor
        .assess(&req.claim, &project)
        .map_err(|e| McpError {
            kind: McpErrorKind::InternalError,
            message: format!("context assessment failed: {e}"),
        })?;
    let context_finding = ContextFinding::from_assessment(context_assessment.clone());
    let t_context = context_start.elapsed().as_millis() as u64;

    // -----------------------------------------------------------------------
    // Context short-circuit: a strong mismatch blocks before any attack work.
    // -----------------------------------------------------------------------
    if context_finding.verdict.is_block() {
        let report = CompletionReport {
            schema_version: SCHEMA_VERSION.to_string(),
            verdict: context_finding.verdict,
            reason_code: context_finding.reason_code.clone(),
            required_action: context_finding.required_action,
            required_action_message: context_finding.required_action_message.clone(),
            request: CompletionRequestInfo {
                repo: req.repo.display().to_string(),
                claim: req.claim.clone(),
                claim_kind: req.claim_kind,
            },
            context: CompletionContextSection {
                status: context_assessment.status,
                verdict: context_finding.verdict,
                reason: context_finding.reason.clone(),
                required_action: context_finding.required_action,
                required_action_message: context_finding.required_action_message.clone(),
                positive_evidence: truncate_evidence(&context_assessment.positive_evidence),
                negative_evidence: truncate_evidence(&context_assessment.negative_evidence),
            },
            assessment: CompletionAssessmentSection {
                initial: None,
                final_assessment: None,
            },
            attacks: CompletionAttackSection {
                planned: Vec::new(),
                results: Vec::new(),
                coverage: None,
                rejected: Vec::new(),
            },
            self_grill: CompletionSelfGrillSection {
                ran: false,
                refuted: false,
                message: None,
                skipped_reason: Some(
                    "context mismatch blocked before adjudication; no M2 decision was produced \
                     for the self-grill to audit"
                        .to_string(),
                ),
                original: None,
                result: None,
            },
            timing: TimingSection {
                context_ms: t_context,
                identity_ms: t_identity,
                evidence_ms: 0,
                adjudication_ms: 0,
                attacks_ms: 0,
                self_grill_ms: 0,
                total_ms: started.elapsed().as_millis() as u64,
                evidence_count: 0,
            },
            // No evidence was collected: the context mismatch blocked before
            // the repository could be inspected.
            repository: None,
        };
        return Ok(report);
    }

    // -----------------------------------------------------------------------
    // Repository state + evidence collection
    // -----------------------------------------------------------------------
    let evidence_start = Instant::now();
    let collectors: Vec<Box<dyn griller_runtime::collectors::Collector>> = vec![
        Box::new(RepositoryStateCollector::new(CollectorEnvironment {
            env: env.clone(),
        })),
        Box::new(SnapshotCollector::new(env.clone())),
        Box::new(GitDiffCollector::new(env.clone())),
    ];
    // A claim naming a specific file/symbol gets that file's content read
    // (when the name is path-like), so existence claims can be resolved with
    // direct evidence instead of remaining unverifiable. A missing file is
    // not a pipeline error: the adjudicator then sees no direct support.
    let input = ClaimInput::new(&req.claim, ClaimSource::Agent).with_kind(req.claim_kind);
    let normalized = griller_adjudicate::normalizer::normalize_deterministic(&input);
    let file_evidence: Vec<Evidence> = normalized
        .target
        .name
        .as_deref()
        .filter(|n| is_path_like(n))
        .map(|name| {
            FileContentCollector {
                relative_path: PathBuf::from(name.trim_start_matches('/')),
            }
            .collect(&req.repo)
            .unwrap_or_default()
        })
        .unwrap_or_default();
    let base_evidence: Vec<Evidence> = {
        let mut evidence: Vec<Evidence> = Vec::new();
        for collector in &collectors {
            match collector.collect(&req.repo) {
                Ok(mut e) => evidence.append(&mut e),
                // Non-git repositories degrade gracefully: git-dependent
                // collectors are skipped, not fatal.
                Err(RuntimeError::Git(_)) => continue,
                Err(e) => {
                    return Err(McpError {
                        kind: McpErrorKind::ExecutionFailed,
                        message: format!("evidence collection failed: {e}"),
                    });
                }
            }
        }
        evidence.extend(file_evidence);
        evidence
    };
    let t_evidence = evidence_start.elapsed().as_millis() as u64;

    // -----------------------------------------------------------------------
    // M2: initial adjudication
    // -----------------------------------------------------------------------
    let adjudication_start = Instant::now();
    let adjudicator = Adjudicator::deterministic();
    let initial_assessment = match adjudicator.assess(&input, &base_evidence, None) {
        Ok(a) => a,
        Err(e) => {
            return Err(McpError {
                kind: McpErrorKind::InternalError,
                message: format!("initial adjudication failed: {e}"),
            });
        }
    };
    let mut t_adjudication = adjudication_start.elapsed().as_millis() as u64;

    // -----------------------------------------------------------------------
    // Dirty working tree: refuse before any attack can produce support.
    //
    // Replay executes in a worktree created with `git worktree add --detach`
    // and no commit-ish, so it inspects `HEAD`. The user's uncommitted edits
    // are not in that worktree. Adjudicating a passing replay as support
    // would therefore let `VERIFIED` describe code the engine never inspected.
    //
    // The dirty state is already collected from the user's tree by
    // `SnapshotCollector` (see the collector loop above). Policy: a dirty
    // repository is UNVERIFIABLE. We do not carry the delta into the worktree
    // and we do not synthesise a working tree.
    // -----------------------------------------------------------------------
    let snapshot = collected_snapshot(&base_evidence);
    let repository = snapshot.and_then(|e| match &e.content {
        EvidenceContent::RepositorySnapshot(s) => Some(CompletionRepositorySection {
            path: req.repo.display().to_string(),
            head: s.snapshot.head.clone(),
            branch: s.snapshot.branch.clone(),
            dirty: s.snapshot.dirty,
            staged_changes: s.snapshot.staged_changes.clone(),
            unstaged_changes: s.snapshot.unstaged_changes.clone(),
            untracked_files: s.snapshot.untracked_files.clone(),
            refusal_reason: None,
        }),
        _ => None,
    });

    if let Some(dirty_state) = repository.as_ref().filter(|r| r.dirty) {
        let refusal = dirty_tree_refusal(dirty_state);
        let snapshot_id = snapshot
            .map(|e| e.id.clone())
            .unwrap_or_else(|| EvidenceId::from_content_hash("dirty-tree"));

        // Same shape `status.rs` produces for UNVERIFIABLE: no support, no
        // contradiction, one insufficiency note naming the blocking evidence.
        // The initial assessment is retained for the record first: it shows
        // what base evidence alone would have concluded, so a refusal that
        // overrides an implied "verified" is auditable.
        let initial_for_report = initial_assessment.clone();
        let final_assessment = griller_adjudicate::assess::ClaimAssessment {
            status: EpistemicStatus::Unverifiable,
            reasons: vec![Reason::RelevantExecutionUnavailable(snapshot_id.clone())],
            supporting_evidence: Vec::new(),
            contradicting_evidence: Vec::new(),
            insufficient: vec![InsufficiencyNote {
                evidence_id: snapshot_id,
                relationship: EvidenceRelationship::Unverifiable,
                reason: refusal.clone(),
            }],
            ..initial_assessment
        };

        let severity = severity_for(&final_assessment.status);
        let policy_context = PolicyContext::new(severity, SeveritySource::Derived);
        let decision = PolicyTable.decide(&final_assessment, &policy_context);

        let mut refused = dirty_state.clone();
        refused.refusal_reason = Some(refusal.clone());

        return Ok(CompletionReport {
            schema_version: SCHEMA_VERSION.to_string(),
            verdict: decision.verdict,
            reason_code: decision.reason_code.clone(),
            required_action: decision.required_action,
            required_action_message: decision.required_action_message.clone(),
            request: CompletionRequestInfo {
                repo: req.repo.display().to_string(),
                claim: req.claim.clone(),
                claim_kind: req.claim_kind,
            },
            context: CompletionContextSection {
                status: context_assessment.status,
                verdict: context_finding.verdict,
                reason: context_finding.reason.clone(),
                required_action: context_finding.required_action,
                required_action_message: context_finding.required_action_message.clone(),
                positive_evidence: truncate_evidence(&context_assessment.positive_evidence),
                negative_evidence: truncate_evidence(&context_assessment.negative_evidence),
            },
            assessment: CompletionAssessmentSection {
                initial: Some(initial_for_report),
                final_assessment: Some(final_assessment),
            },
            attacks: CompletionAttackSection {
                planned: Vec::new(),
                results: Vec::new(),
                coverage: Some(build_coverage(&[], &[], &[])),
                rejected: vec![format!("dirty working tree: {refusal}")],
            },
            self_grill: CompletionSelfGrillSection {
                ran: false,
                refuted: false,
                message: None,
                skipped_reason: Some(
                    "repository working tree was dirty; verification was refused before any \
                     attack ran, so there was no M2 decision for the self-grill to audit"
                        .to_string(),
                ),
                original: None,
                result: None,
            },
            timing: TimingSection {
                context_ms: t_context,
                identity_ms: t_identity,
                evidence_ms: t_evidence,
                adjudication_ms: t_adjudication,
                attacks_ms: 0,
                self_grill_ms: 0,
                total_ms: started.elapsed().as_millis() as u64,
                evidence_count: base_evidence.len(),
            },
            repository: Some(refused),
        });
    }

    // -----------------------------------------------------------------------
    // M5: plan applicable safe attacks, then execute them
    // -----------------------------------------------------------------------
    let attacks_start = Instant::now();
    let planned_kinds = plan::applicable_attacks(&req.claim, &req.repo);

    let mut planned: Vec<AttackSpecSummary> = Vec::new();
    let mut results: Vec<AttackOutcomeInfo> = Vec::new();
    let mut rejected: Vec<String> = Vec::new();
    let mut failed_kinds: Vec<AttackKind> = Vec::new();
    let mut combined_evidence: Vec<Evidence> = base_evidence.clone();

    for kind in &planned_kinds {
        let kind = *kind;
        let mut spec = AttackSpec {
            id: griller_core::attack::AttackId::from_content_hash(&format!(
                "completion-{}-{}",
                kind, req.claim
            )),
            kind,
            target_claim: req.claim.clone(),
            command: None,
            working_state: None,
            timeout_ms: req.timeout_ms,
            rationale: Some(format!("Completion check: {}", kind)),
        };
        if kind == AttackKind::Replay {
            let mut cmd = plan::derive_replay_command(&req.claim, &req.repo);
            if let Some(c) = cmd.as_mut() {
                c.working_directory = Some(req.repo.clone());
            }
            spec.command = cmd;
        }

        let validation = AttackExecutor::validate(&spec);
        if !validation.allowed {
            rejected.push(format!("{}: {}", kind, validation.reason));
            failed_kinds.push(kind);
            continue;
        }

        planned.push(AttackSpecSummary {
            id: spec.id.to_string(),
            kind,
            target_claim: spec.target_claim.clone(),
            timeout_ms: spec.timeout_ms,
            rationale: spec.rationale.clone(),
        });

        if kind == AttackKind::Replay {
            // Single execution: the replay is captured once as command-output
            // evidence; the attack outcome is derived from that same run.
            // (Executing separately for outcome and evidence would run the
            // command twice in two cold worktrees.)
            match replay_evidence(&spec, &req.repo, &env) {
                Ok(ev) => {
                    let (outcome, explanation) = replay_outcome_of(&ev);
                    let evidence_refs = vec![ev.id.to_string()];
                    combined_evidence.push(ev);
                    results.push(AttackOutcomeInfo {
                        attack_id: spec.id.to_string(),
                        kind,
                        outcome,
                        explanation,
                        evidence_refs,
                    });
                }
                Err(e) => {
                    rejected.push(format!("{}: {e}", kind));
                    failed_kinds.push(kind);
                }
            }
            continue;
        }

        match execute_attack(&spec, &req.repo, &env) {
            Ok(result) => {
                results.push(AttackOutcomeInfo {
                    attack_id: spec.id.to_string(),
                    kind,
                    outcome: result.outcome,
                    explanation: result.explanation,
                    evidence_refs: result.evidence_refs.iter().map(|e| e.to_string()).collect(),
                });
            }
            Err(e) => {
                rejected.push(format!("{}: {e}", kind));
                failed_kinds.push(kind);
            }
        }
    }
    let t_attacks = attacks_start.elapsed().as_millis() as u64;

    // Coverage report (M2 canonical type) from the actual plan + results.
    let coverage = build_coverage(&results, &failed_kinds, &planned_kinds);

    // -----------------------------------------------------------------------
    // M2: re-adjudication with attack evidence
    // -----------------------------------------------------------------------
    let readjudication_start = Instant::now();
    let final_assessment = match adjudicator.assess(&input, &combined_evidence, None) {
        Ok(a) => a,
        Err(e) => {
            return Err(McpError {
                kind: McpErrorKind::InternalError,
                message: format!("final adjudication failed: {e}"),
            });
        }
    };
    t_adjudication += readjudication_start.elapsed().as_millis() as u64;

    // -----------------------------------------------------------------------
    // M3: policy decision on the final assessment
    // -----------------------------------------------------------------------
    let severity = severity_for(&final_assessment.status);
    let policy_context = PolicyContext::new(severity, SeveritySource::Derived);
    let decision = PolicyTable.decide(&final_assessment, &policy_context);

    // -----------------------------------------------------------------------
    // M7: self-grill on the real pipeline data
    // -----------------------------------------------------------------------
    let self_grill_start = Instant::now();
    let sg_input = griller_self_grill::SelfGrillInput::from_pipeline(
        &final_assessment,
        &decision,
        Some(&context_assessment),
        Some(&coverage),
        &combined_evidence,
        severity,
        SeveritySource::Derived,
    );
    let (original, sg_result) = griller_self_grill::self_grill(&sg_input);
    let t_self_grill = self_grill_start.elapsed().as_millis() as u64;

    let sg_refuted = sg_result.outcome.is_refuted();
    let sg_section = CompletionSelfGrillSection {
        ran: true,
        refuted: sg_refuted,
        message: sg_refuted.then(|| {
            "Self-grill found a structural inconsistency in the original decision.".to_string()
        }),
        skipped_reason: None,
        original: Some(original),
        result: Some(sg_result),
    };

    // -----------------------------------------------------------------------
    // Report
    // -----------------------------------------------------------------------
    Ok(CompletionReport {
        schema_version: SCHEMA_VERSION.to_string(),
        verdict: decision.verdict,
        reason_code: decision.reason_code.clone(),
        required_action: decision.required_action,
        required_action_message: decision.required_action_message.clone(),
        request: CompletionRequestInfo {
            repo: req.repo.display().to_string(),
            claim: req.claim.clone(),
            claim_kind: req.claim_kind,
        },
        context: CompletionContextSection {
            status: context_assessment.status,
            verdict: context_finding.verdict,
            reason: context_finding.reason.clone(),
            required_action: context_finding.required_action,
            required_action_message: context_finding.required_action_message.clone(),
            positive_evidence: truncate_evidence(&context_assessment.positive_evidence),
            negative_evidence: truncate_evidence(&context_assessment.negative_evidence),
        },
        assessment: CompletionAssessmentSection {
            initial: Some(initial_assessment),
            final_assessment: Some(final_assessment),
        },
        attacks: CompletionAttackSection {
            planned,
            results,
            coverage: Some(coverage),
            rejected,
        },
        self_grill: sg_section,
        timing: TimingSection {
            context_ms: t_context,
            identity_ms: t_identity,
            evidence_ms: t_evidence,
            adjudication_ms: t_adjudication,
            attacks_ms: t_attacks,
            self_grill_ms: t_self_grill,
            total_ms: started.elapsed().as_millis() as u64,
            evidence_count: combined_evidence.len(),
        },
        repository,
    })
}

/// Build the canonical M2 attack-coverage report from what actually happened.
///
/// Rejected or failed attacks are execution failures for coverage purposes:
/// they never count as supporting evidence and block ADVERSARIALLY_VERIFIED.
fn build_coverage(
    results: &[AttackOutcomeInfo],
    failed_kinds: &[AttackKind],
    planned_kinds: &[AttackKind],
) -> M2AttackCoverage {
    let mut coverage = M2AttackCoverage::new(planned_kinds.len());

    for r in results {
        coverage.observe(r.kind, r.outcome, true);
    }
    for kind in failed_kinds {
        coverage.observe(*kind, AttackOutcome::ExecutionFailed, true);
    }
    coverage
}

/// A summary of one planned attack (for transparency in the report).
#[derive(Debug, Serialize, Clone)]
pub struct AttackSpecSummary {
    pub id: String,
    pub kind: AttackKind,
    pub target_claim: String,
    pub timeout_ms: u64,
    pub rationale: Option<String>,
}

/// The outcome of one executed attack, with evidence references.
#[derive(Debug, Serialize, Clone)]
pub struct AttackOutcomeInfo {
    pub attack_id: String,
    pub kind: AttackKind,
    pub outcome: AttackOutcome,
    pub explanation: String,
    pub evidence_refs: Vec<String>,
}

/// Execute one attack spec against the repository. Returns an error string
/// when the attack could not be planned/executed (never treated as refutation).
pub fn execute_attack(
    spec: &AttackSpec,
    repo: &Path,
    env: &EnvSource,
) -> Result<AttackResult, String> {
    use AttackKind::*;
    match spec.kind {
        Replay => {
            if spec.command.is_none() {
                return Err("replay requires a command".into());
            }
            // Serialize replays across processes: two simultaneous gate runs
            // must not race their verification suites (shared fixture state,
            // ports), or each can refute the other non-deterministically.
            let _replay_lock =
                ReplayLock::acquire(repo, std::time::Duration::from_millis(REPLAY_TIMEOUT_MS));
            // Replay executes in an isolated worktree so the user's working
            // tree is never mutated (in-place fallback for non-git paths).
            // The worktree stays alive until execution completes.
            let worktree = match Worktree::create(repo) {
                Ok(wt) => Some(wt),
                Err(RuntimeError::Git(_)) => None,
                Err(e) => return Err(e.to_string()),
            };
            let mut spec = spec.clone();
            if let Some(cmd) = spec.command.as_mut() {
                cmd.working_directory = Some(
                    worktree
                        .as_ref()
                        .map(|wt| wt.path().to_path_buf())
                        .unwrap_or_else(|| repo.to_path_buf()),
                );
            }
            let result =
                AttackExecutor::execute_replay(&spec, repo, env).map_err(|e| e.to_string());
            drop(worktree);
            result
        }
        PresenceAbsence => {
            AttackExecutor::execute_presence_absence(spec, repo).map_err(|e| e.to_string())
        }
        StubSweep => AttackExecutor::execute_stub_sweep(spec, repo).map_err(|e| e.to_string()),
        Safety => AttackExecutor::execute_safety(spec, repo).map_err(|e| e.to_string()),
        Completeness => AttackExecutor::execute_completeness(spec, repo).map_err(|e| e.to_string()),
        Invert => AttackExecutor::execute_invert(spec, repo).map_err(|e| e.to_string()),
        Robustness => {
            AttackExecutor::execute_robustness(spec, repo, env).map_err(|e| e.to_string())
        }
        RequirementCoverage => {
            AttackExecutor::execute_requirement_coverage(spec, &[]).map_err(|e| e.to_string())
        }
    }
}

/// Run a command in an isolated worktree and capture it as evidence.
///
/// Falls back to running in the repository directory only when worktree
/// creation is impossible (non-git path). The user's working tree is never
/// mutated.
fn replay_evidence(spec: &AttackSpec, repo: &Path, env: &EnvSource) -> Result<Evidence, McpError> {
    let cmd = spec.command.clone().ok_or_else(|| McpError {
        kind: McpErrorKind::InternalError,
        message: "replay evidence requires a command".into(),
    })?;
    // Same cross-process serialization as the attack path.
    let _replay_lock =
        ReplayLock::acquire(repo, std::time::Duration::from_millis(REPLAY_TIMEOUT_MS));
    run_command_in_isolation(&cmd, repo, env)
}

/// Derive the replay attack outcome from its captured evidence.
///
/// Mirrors `AttackExecutor::outcome_for_status` so one execution can serve
/// both as M2 evidence and as the M5 attack result.
fn replay_outcome_of(ev: &Evidence) -> (AttackOutcome, String) {
    match ev.content.as_command_output() {
        Some(out) => match out.outcome() {
            griller_core::command::CommandOutcome::Success => (
                AttackOutcome::NoObjection,
                format!("replay succeeded (exit 0, {}ms)", out.duration_ms),
            ),
            griller_core::command::CommandOutcome::Failed => (
                AttackOutcome::Refuted,
                format!(
                    "replay failed (exit {}, {}ms)",
                    out.exit_code.unwrap_or(-1),
                    out.duration_ms
                ),
            ),
            _ => (
                AttackOutcome::ExecutionFailed,
                "replay could not execute; the outcome is undetermined".to_string(),
            ),
        },
        None => (
            AttackOutcome::ExecutionFailed,
            "replay evidence is not command output".to_string(),
        ),
    }
}

fn run_command_in_isolation(
    cmd: &CommandSpec,
    repo: &Path,
    env: &EnvSource,
) -> Result<Evidence, McpError> {
    let collector = CommandExecutionCollector::new(env.clone());

    let worktree = Worktree::create(repo);
    match worktree {
        Ok(wt) => {
            let dir = wt.path().to_path_buf();
            let result = collector.collect(cmd, &dir);
            let _ = wt.remove();
            result.map_err(|e| McpError {
                kind: McpErrorKind::ExecutionFailed,
                message: format!("isolated replay execution failed: {e}"),
            })
        }
        Err(RuntimeError::Git(_)) => {
            // Not a git repository: worktree isolation is impossible. Run
            // in-place (read-only command) as the documented fallback.
            collector.collect(cmd, repo).map_err(|e| McpError {
                kind: McpErrorKind::ExecutionFailed,
                message: format!("replay execution failed: {e}"),
            })
        }
        Err(e) => Err(McpError {
            kind: McpErrorKind::ExecutionFailed,
            message: format!("worktree creation failed: {e}"),
        }),
    }
}

fn truncate_evidence<T: Clone>(items: &[T]) -> Vec<T> {
    items.iter().take(MAX_EVIDENCE_SNIPPETS).cloned().collect()
}

fn severity_for(status: &EpistemicStatus) -> Severity {
    match status {
        EpistemicStatus::Verified | EpistemicStatus::AdversariallyVerified => Severity::Info,
        EpistemicStatus::Unsupported | EpistemicStatus::Unverifiable => Severity::High,
        EpistemicStatus::Uncertain => Severity::Medium,
        EpistemicStatus::Contradicted | EpistemicStatus::False => Severity::High,
        _ => Severity::Medium,
    }
}

// ---------------------------------------------------------------------------
// Report model
// ---------------------------------------------------------------------------

/// The canonical `check_completion` report.
///
/// Decision fields come first so an agent does not have to parse a long
/// narrative to find PASS/WARN/BLOCK. Evidence payloads are never duplicated;
/// every stage carries references instead.
#[derive(Debug, Serialize)]
pub struct CompletionReport {
    pub schema_version: String,
    /// The operational verdict: PASS, WARN or BLOCK.
    pub verdict: Verdict,
    pub reason_code: String,
    pub required_action: RequiredAction,
    pub required_action_message: String,
    pub request: CompletionRequestInfo,
    /// M4: does this task/claim belong to this repository?
    pub context: CompletionContextSection,
    /// M2: initial and re-adjudicated epistemic status.
    pub assessment: CompletionAssessmentSection,
    /// M5: planned/executed attacks and coverage.
    pub attacks: CompletionAttackSection,
    /// M7: self-grill audit of the final decision (never overrides it).
    pub self_grill: CompletionSelfGrillSection,
    /// Per-stage timing diagnostics (descriptive, not confidence).
    pub timing: TimingSection,
    /// The repository state this verdict describes.
    ///
    /// `Some` whenever the repository could be inspected as a git repository.
    /// `dirty = true` means the working tree differed from `HEAD`; in that case
    /// verification is refused and `refusal_reason` explains why, because
    /// replay would have inspected `HEAD` rather than the working tree.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository: Option<CompletionRepositorySection>,
}

/// The base + working state of the repository a verdict was produced for.
///
/// Paths and porcelain lines only — never file contents, never secrets.
#[derive(Debug, Serialize, Clone)]
pub struct CompletionRepositorySection {
    /// The repository path the request was made against.
    pub path: String,
    /// `HEAD` at verification time.
    pub head: Option<String>,
    /// Branch name when not detached.
    pub branch: Option<String>,
    /// True when staged, unstaged or untracked changes existed.
    pub dirty: bool,
    /// Porcelain lines for staged changes.
    pub staged_changes: Vec<String>,
    /// Porcelain lines for unstaged changes.
    pub unstaged_changes: Vec<String>,
    /// Untracked file paths.
    pub untracked_files: Vec<String>,
    /// Why verification was refused; `Some` only when `dirty` is true.
    pub refusal_reason: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CompletionRequestInfo {
    pub repo: String,
    pub claim: String,
    pub claim_kind: ClaimKind,
}

/// M4 section of the report.
#[derive(Debug, Serialize)]
pub struct CompletionContextSection {
    pub status: ContextStatus,
    pub verdict: Verdict,
    pub reason: String,
    pub required_action: RequiredAction,
    pub required_action_message: String,
    pub positive_evidence: Vec<griller_context::evidence::ContextEvidence>,
    pub negative_evidence: Vec<griller_context::evidence::ContextEvidence>,
}

/// M2 section of the report.
#[derive(Debug, Serialize)]
pub struct CompletionAssessmentSection {
    pub initial: Option<griller_adjudicate::assess::ClaimAssessment>,
    #[serde(rename = "final")]
    pub final_assessment: Option<griller_adjudicate::assess::ClaimAssessment>,
}

/// M5 section of the report.
#[derive(Debug, Serialize)]
pub struct CompletionAttackSection {
    pub planned: Vec<AttackSpecSummary>,
    pub results: Vec<AttackOutcomeInfo>,
    pub coverage: Option<M2AttackCoverage>,
    pub rejected: Vec<String>,
}

/// M7 section of the report.
///
/// The original decision remains immutable; self-grill is an audit layer that
/// never silently converts BLOCK->PASS or PASS->BLOCK. When a structural
/// inconsistency is found, `refuted` is set and the message explains it.
#[derive(Debug, Serialize)]
pub struct CompletionSelfGrillSection {
    pub ran: bool,
    pub refuted: bool,
    pub message: Option<String>,
    pub skipped_reason: Option<String>,
    pub original: Option<griller_self_grill::OriginalDecision>,
    pub result: Option<griller_self_grill::SelfGrillResult>,
}

/// The repository-snapshot evidence collected from the user's working tree,
/// when the target is a git repository.
///
/// `SnapshotCollector` reads `req.repo` — the user's tree, not the isolated
/// replay worktree — so this is the only place the pipeline observes whether
/// the claimed state is actually the state that would be verified.
fn collected_snapshot(evidence: &[Evidence]) -> Option<&Evidence> {
    evidence
        .iter()
        .find(|e| matches!(e.content, EvidenceContent::RepositorySnapshot(_)))
}

/// Explain why a dirty working tree cannot be verified.
///
/// Deterministic, contains no claim text, no file contents and no secrets: only
/// counts and the `HEAD` identifier already present in the snapshot.
fn dirty_tree_refusal(state: &CompletionRepositorySection) -> String {
    let head = state.head.as_deref().unwrap_or("(no commits)");
    format!(
        "Repository working tree is dirty at {head} ({} staged, {} unstaged, {} untracked). \
         Replay executes in an isolated worktree checked out from HEAD, so it would verify the \
         committed state rather than the agent's uncommitted working tree. Verification refused \
         to prevent a VERIFIED result for code that was not inspected; commit or stash the \
         working tree changes and re-run.",
        state.staged_changes.len(),
        state.unstaged_changes.len(),
        state.untracked_files.len(),
    )
}

/// Per-stage wall-clock timing in milliseconds.
#[derive(Debug, Serialize)]
pub struct TimingSection {
    pub context_ms: u64,
    pub identity_ms: u64,
    pub evidence_ms: u64,
    pub adjudication_ms: u64,
    pub attacks_ms: u64,
    pub self_grill_ms: u64,
    pub total_ms: u64,
    pub evidence_count: usize,
}

use serde::Serialize;
/// Whether a claim target name looks like a repository-relative file path.
fn is_path_like(name: &str) -> bool {
    name.contains('/')
        || name.ends_with(".rs")
        || name.ends_with(".py")
        || name.ends_with(".js")
        || name.ends_with(".ts")
        || name.ends_with(".toml")
        || name.ends_with(".json")
        || name.ends_with(".md")
}
