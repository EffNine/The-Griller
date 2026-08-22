//! `griller-mcp`: MCP adapter for The Griller.
//!
//! Exposes exactly six public tools over stdio:
//! * `griller.health` — server capabilities and security boundaries
//! * `griller.verify_claim` — verify a claim against repository evidence
//! * `griller.check_context` — determine whether a task belongs to this repo
//! * `griller.run_attacks` — run adversarial verification attacks
//! * `griller.check_completion` — the M8 golden path (M4 -> M2 -> M5 -> M2 -> M3 -> M7)
//! * `griller.self_grill` — M7 standalone self-audit
//!
//! MCP is an adapter: handlers translate requests into existing Griller APIs.
//! No business logic lives in this crate beyond the orchestration of the
//! `check_completion` pipeline (see the [`completion`] module).

#![forbid(unsafe_code)]
#![allow(missing_docs)]

mod completion;
mod error;
mod plan;
mod validate;

pub use completion::{
    run_completion_check, AttackOutcomeInfo, AttackSpecSummary, CompletionReport,
    CompletionRequest, SCHEMA_VERSION,
};
pub use error::{error_result, McpError, McpErrorKind};
pub use validate::{
    validate_repo_path, validate_text, validate_timeout, MAX_ATTACKS, MAX_EVIDENCE_SNIPPETS,
    MAX_TEXT_LEN,
};

use std::sync::Arc;

use async_trait::async_trait;
use griller_adjudicate::{
    assess::Adjudicator, claim::ClaimInput, ClaimKind, ClaimSource, ClaimTarget, EpistemicStatus,
    TargetScope,
};
use griller_context::assess::ContextAssessor;
use griller_core::evidence::Evidence;
use griller_policy::{
    context::ContextFinding,
    decision::RequiredAction,
    policy::{Policy, PolicyContext, PolicyInput, PolicyTable},
    severity::Severity,
    verdict::Verdict,
};
use rust_mcp_sdk::{
    error::SdkResult,
    macros::{self, JsonSchema},
    mcp_server::{server_runtime, McpServerOptions, ServerHandler},
    schema::{
        schema_utils::CallToolError, CallToolResult, Implementation, InitializeResult,
        ListResourcesResult, ListToolsResult, PaginatedRequestParams, RpcError, TextContent,
    },
    ToMcpServerHandler, *,
};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Health tool
// ---------------------------------------------------------------------------

#[macros::mcp_tool(
    name = "griller.health",
    description = "Return server capabilities, security boundaries, and version information."
)]
#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
pub struct HealthTool {}

impl HealthTool {
    pub fn call_tool(&self) -> Result<CallToolResult, CallToolError> {
        let health = HealthResponse {
            service: "the-griller".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            protocol: "2025-11-25".to_string(),
            schema_version: "m8.0".to_string(),
            tools: vec![
                "griller.verify_claim".to_string(),
                "griller.check_completion".to_string(),
                "griller.check_context".to_string(),
                "griller.run_attacks".to_string(),
                "griller.self_grill".to_string(),
                "griller.health".to_string(),
            ],
            capabilities: Capabilities {
                claim_verification: true,
                context_check: true,
                red_team: true,
                self_grill: true,
            },
            execution: ExecutionCapabilities {
                shell_free: true,
                worktree_isolation: true,
                network_isolation: false,
                os_sandbox: false,
            },
            security: SecurityInfo {
                shell_free: true,
                command_allow_list: true,
                worktree_isolation: true,
                secret_redaction: "best_effort".to_string(),
                network_isolation: "not_enforced".to_string(),
                os_sandbox: "not_enforced".to_string(),
                privilege_reduction: "not_enforced".to_string(),
            },
        };
        let json = serde_json::to_string_pretty(&health)
            .map_err(|e| CallToolError::from_message(format!("serialization error: {e}")))?;
        Ok(CallToolResult::text_content(vec![TextContent::from(json)]))
    }
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub service: String,
    pub version: String,
    pub protocol: String,
    pub schema_version: String,
    pub tools: Vec<String>,
    pub capabilities: Capabilities,
    pub execution: ExecutionCapabilities,
    pub security: SecurityInfo,
}

#[derive(Debug, Serialize)]
pub struct Capabilities {
    pub claim_verification: bool,
    pub context_check: bool,
    pub red_team: bool,
    pub self_grill: bool,
}

#[derive(Debug, Serialize)]
pub struct ExecutionCapabilities {
    pub shell_free: bool,
    pub worktree_isolation: bool,
    pub network_isolation: bool,
    pub os_sandbox: bool,
}

#[derive(Debug, Serialize)]
pub struct SecurityInfo {
    pub shell_free: bool,
    pub command_allow_list: bool,
    pub worktree_isolation: bool,
    pub secret_redaction: String,
    pub network_isolation: String,
    pub os_sandbox: String,
    pub privilege_reduction: String,
}

// ---------------------------------------------------------------------------
// verify_claim tool
// ---------------------------------------------------------------------------

#[macros::mcp_tool(
    name = "griller.verify_claim",
    description = "Independently verify a claim against repository evidence. Agent narration is not treated as verification."
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct VerifyClaimTool {
    pub repo: String,
    pub claim: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claim_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,
    #[serde(default = "default_evidence_mode")]
    pub evidence_mode: String,
}

fn default_evidence_mode() -> String {
    "minimal".to_string()
}

impl VerifyClaimTool {
    pub fn call_tool(&self) -> Result<CallToolResult, CallToolError> {
        let repo = match validate_repo_path(&self.repo) {
            Ok(r) => r,
            Err(e) => return Ok(CallToolResult::from(e)),
        };
        let claim = match validate_text(&self.claim, "claim") {
            Ok(c) => c,
            Err(e) => return Ok(CallToolResult::from(e)),
        };
        let claim_kind = parse_claim_kind(&self.claim_kind);
        let target = parse_target(&self.target);
        let severity = parse_severity(&self.severity);

        let env = griller_runtime::execution::EnvSource::from_host();
        let collectors = Self::collectors(&env);
        let evidence = match collectors
            .iter()
            .map(|c| c.collect(&repo))
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(e) => e.into_iter().flatten().collect::<Vec<_>>(),
            Err(e) => {
                return Ok(error_result(
                    McpErrorKind::ExecutionFailed,
                    &format!("evidence collection failed: {e}"),
                ));
            }
        };

        let input = ClaimInput::new(&claim, ClaimSource::Agent)
            .with_kind(claim_kind)
            .with_target(target);
        let adjudicator = Adjudicator::deterministic();
        let assessment = match adjudicator.assess(&input, &evidence, None) {
            Ok(a) => a,
            Err(e) => {
                return Ok(error_result(
                    McpErrorKind::InternalError,
                    &format!("adjudication failed: {e}"),
                ));
            }
        };

        let sev = severity.unwrap_or(match assessment.status {
            EpistemicStatus::Verified | EpistemicStatus::AdversariallyVerified => Severity::Info,
            EpistemicStatus::Unsupported | EpistemicStatus::Unverifiable => Severity::High,
            EpistemicStatus::Uncertain => Severity::Medium,
            EpistemicStatus::Contradicted | EpistemicStatus::False => Severity::High,
            _ => Severity::Medium,
        });
        let policy_input = PolicyInput {
            assessment: assessment.clone(),
            context: PolicyContext::new(sev, griller_policy::severity::SeveritySource::Declared),
        };
        let policy = PolicyTable;
        let finding = policy.evaluate(&policy_input);

        let evidence_refs: Vec<String> = assessment
            .supporting_evidence
            .iter()
            .chain(&assessment.contradicting_evidence)
            .take(MAX_EVIDENCE_SNIPPETS)
            .map(|id| id.as_str().to_string())
            .collect();

        let result = VerifyClaimResponse {
            claim,
            assessment,
            evidence_refs,
            severity: finding.severity,
            verdict: finding.verdict,
            reason: finding.reason,
            required_action: finding.required_action,
            required_action_message: finding.required_action_message,
        };

        let json = serde_json::to_string_pretty(&result)
            .map_err(|e| CallToolError::from_message(format!("serialization error: {e}")))?;
        Ok(CallToolResult::text_content(vec![TextContent::from(json)]))
    }

    fn collectors(
        env: &griller_runtime::execution::EnvSource,
    ) -> Vec<Box<dyn griller_runtime::collectors::Collector>> {
        vec![
            Box::new(griller_runtime::collectors::RepositoryStateCollector::new(
                griller_runtime::collectors::CollectorEnvironment { env: env.clone() },
            )),
            Box::new(griller_runtime::collectors::SnapshotCollector::new(
                env.clone(),
            )),
        ]
    }
}

#[derive(Debug, Serialize)]
pub struct VerifyClaimResponse {
    pub claim: String,
    pub assessment: griller_adjudicate::assess::ClaimAssessment,
    pub evidence_refs: Vec<String>,
    pub severity: Severity,
    pub verdict: Verdict,
    pub reason: String,
    pub required_action: RequiredAction,
    pub required_action_message: String,
}

// ---------------------------------------------------------------------------
// check_context tool
// ---------------------------------------------------------------------------

#[macros::mcp_tool(
    name = "griller.check_context",
    description = "Determine whether a task appears to target the current repository, including explicit foreign-project mismatch detection."
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct CheckContextTool {
    pub repo: String,
    pub task: String,
}

impl CheckContextTool {
    pub fn call_tool(&self) -> Result<CallToolResult, CallToolError> {
        let repo = match validate_repo_path(&self.repo) {
            Ok(r) => r,
            Err(e) => return Ok(CallToolResult::from(e)),
        };
        let task = match validate_text(&self.task, "task") {
            Ok(t) => t,
            Err(e) => return Ok(CallToolResult::from(e)),
        };

        let env = griller_runtime::execution::EnvSource::from_host();
        let state_collector = griller_runtime::collectors::RepositoryStateCollector::new(
            griller_runtime::collectors::CollectorEnvironment { env: env.clone() },
        );
        let _collected = match state_collector.collect_context(&repo) {
            Ok(c) => c,
            Err(e) => {
                return Ok(error_result(
                    McpErrorKind::ExecutionFailed,
                    &format!("context collection failed: {e}"),
                ));
            }
        };

        let identity_collector = griller_runtime::identity::ProjectIdentityCollector::new(
            griller_runtime::collectors::CollectorEnvironment { env },
        );
        let project = match identity_collector.collect(&repo) {
            Ok(p) => p,
            Err(e) => {
                return Ok(error_result(
                    McpErrorKind::ExecutionFailed,
                    &format!("identity collection failed: {e}"),
                ));
            }
        };

        let assessor = ContextAssessor::deterministic();
        let assessment = match assessor.assess(&task, &project) {
            Ok(a) => a,
            Err(e) => {
                return Ok(error_result(
                    McpErrorKind::InternalError,
                    &format!("context assessment failed: {e}"),
                ));
            }
        };

        let finding = ContextFinding::from_assessment(assessment.clone());

        let result = CheckContextResponse {
            task_identity: assessment.task,
            project_identity: assessment.project,
            context_status: assessment.status,
            positive_evidence: assessment
                .positive_evidence
                .iter()
                .take(MAX_EVIDENCE_SNIPPETS)
                .cloned()
                .collect(),
            negative_evidence: assessment
                .negative_evidence
                .iter()
                .take(MAX_EVIDENCE_SNIPPETS)
                .cloned()
                .collect(),
            severity: finding.severity,
            verdict: finding.verdict,
            reason: finding.reason,
            required_action: finding.required_action,
            required_action_message: finding.required_action_message,
        };

        let json = serde_json::to_string_pretty(&result)
            .map_err(|e| CallToolError::from_message(format!("serialization error: {e}")))?;
        Ok(CallToolResult::text_content(vec![TextContent::from(json)]))
    }
}

#[derive(Debug, Serialize)]
pub struct CheckContextResponse {
    pub task_identity: griller_context::task::TaskIdentity,
    pub project_identity: griller_context::project::ProjectIdentity,
    pub context_status: griller_context::status::ContextStatus,
    pub positive_evidence: Vec<griller_context::evidence::ContextEvidence>,
    pub negative_evidence: Vec<griller_context::evidence::ContextEvidence>,
    pub severity: Severity,
    pub verdict: Verdict,
    pub reason: String,
    pub required_action: RequiredAction,
    pub required_action_message: String,
}

// ---------------------------------------------------------------------------
// run_attacks tool
// ---------------------------------------------------------------------------

#[macros::mcp_tool(
    name = "griller.run_attacks",
    description = "Run applicable safe adversarial verification attacks against a claim. Attack kinds are validated strictly: unknown kinds are rejected, never silently dropped."
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct RunAttacksTool {
    pub repo: String,
    pub claim: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claim_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attack_kinds: Option<Vec<String>>,
    #[serde(default)]
    pub timeout: u64,
}

impl RunAttacksTool {
    pub fn call_tool(&self) -> Result<CallToolResult, CallToolError> {
        let repo = match validate_repo_path(&self.repo) {
            Ok(r) => r,
            Err(e) => return Ok(CallToolResult::from(e)),
        };
        let claim = match validate_text(&self.claim, "claim") {
            Ok(c) => c,
            Err(e) => return Ok(CallToolResult::from(e)),
        };
        let timeout = match validate_timeout(self.timeout) {
            Ok(t) => t,
            Err(e) => return Ok(CallToolResult::from(e)),
        };
        let _claim_kind = parse_claim_kind(&self.claim_kind);
        let _target = parse_target(&self.target);

        let selected_kinds: Vec<griller_core::attack::AttackKind> =
            match self.attack_kinds.as_deref() {
                Some(kinds) => match plan::validate_attack_kinds(kinds) {
                    Ok(k) => k,
                    Err(e) => return Ok(CallToolResult::from(e)),
                },
                None => plan::applicable_attacks(&claim, &repo),
            };

        let env = griller_runtime::execution::EnvSource::from_host();
        let mut specs: Vec<griller_core::attack::AttackSpec> = Vec::new();
        for kind in selected_kinds {
            let mut spec = griller_core::attack::AttackSpec {
                id: griller_core::attack::AttackId::from_content_hash(&format!(
                    "{}-{}",
                    kind, claim
                )),
                kind,
                target_claim: claim.clone(),
                command: None,
                working_state: None,
                timeout_ms: timeout,
                rationale: Some(format!("Adversarial check: {} against '{}'", kind, claim)),
            };
            if kind == griller_core::attack::AttackKind::Replay {
                spec.command = plan::derive_replay_command(&claim, &repo);
            }
            specs.push(spec);
        }

        let mut results: Vec<AttackOutcomeInfo> = Vec::new();
        let mut execution_errors: Vec<String> = Vec::new();

        for spec in &specs {
            let validation = griller_runtime::attack::AttackExecutor::validate(spec);
            if !validation.allowed {
                execution_errors.push(format!("rejected: {}", validation.reason));
                continue;
            }
            if !griller_runtime::attack::AttackValidator::is_attack_implemented(&spec.kind) {
                continue;
            }
            let result = match crate::completion::execute_attack(spec, &repo, &env) {
                Ok(r) => r,
                Err(e) => {
                    execution_errors.push(format!("{}: {e}", spec.kind));
                    continue;
                }
            };
            results.push(AttackOutcomeInfo {
                attack_id: spec.id.to_string(),
                kind: spec.kind,
                outcome: result.outcome,
                explanation: result.explanation,
                evidence_refs: result.evidence_refs.iter().map(|e| e.to_string()).collect(),
            });
        }

        let attack_coverage = AttackCoverage {
            total: specs.len(),
            executed: results.len(),
            rejected: execution_errors.len(),
            refuted: results.iter().filter(|r| r.outcome.is_refuted()).count(),
            confirmed: results.iter().filter(|r| r.outcome.is_confirmed()).count(),
            no_objection: results.iter().filter(|r| r.outcome.is_supporting()).count(),
            inconclusive: results
                .iter()
                .filter(|r| r.outcome.is_undetermined())
                .count(),
        };

        let result = RunAttacksResponse {
            attack_specs: specs
                .iter()
                .map(|s| AttackSpecSummary {
                    id: s.id.to_string(),
                    kind: s.kind,
                    target_claim: s.target_claim.clone(),
                    timeout_ms: s.timeout_ms,
                    rationale: s.rationale.clone(),
                })
                .collect(),
            attack_results: results,
            attack_coverage: attack_coverage.clone(),
            execution_errors,
        };

        let json = serde_json::to_string_pretty(&result)
            .map_err(|e| CallToolError::from_message(format!("serialization error: {e}")))?;
        Ok(CallToolResult::text_content(vec![TextContent::from(json)]))
    }
}

#[derive(Debug, Serialize)]
pub struct RunAttacksResponse {
    pub attack_specs: Vec<AttackSpecSummary>,
    pub attack_results: Vec<AttackOutcomeInfo>,
    pub attack_coverage: AttackCoverage,
    pub execution_errors: Vec<String>,
}

/// Summary coverage for the run_attacks tool.
#[derive(Debug, Serialize, Clone)]
pub struct AttackCoverage {
    pub total: usize,
    pub executed: usize,
    pub rejected: usize,
    pub refuted: usize,
    pub confirmed: usize,
    pub no_objection: usize,
    pub inconclusive: usize,
}

// ---------------------------------------------------------------------------
// check_completion tool
// ---------------------------------------------------------------------------

#[macros::mcp_tool(
    name = "griller.check_completion",
    description = "Independently evaluate an agent's completion claim using repository context, evidence collection, deterministic adjudication, applicable safe adversarial checks, and a self-grill audit of the resulting decision."
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct CheckCompletionTool {
    pub repo: String,
    pub claim: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claim_kind: Option<String>,
    #[serde(default)]
    pub timeout: u64,
}

impl CheckCompletionTool {
    pub fn call_tool(&self) -> Result<CallToolResult, CallToolError> {
        let repo = match validate_repo_path(&self.repo) {
            Ok(r) => r,
            Err(e) => return Ok(CallToolResult::from(e)),
        };
        let claim = match validate_text(&self.claim, "claim") {
            Ok(c) => c,
            Err(e) => return Ok(CallToolResult::from(e)),
        };
        let timeout = match validate_timeout(self.timeout) {
            Ok(t) => t,
            Err(e) => return Ok(CallToolResult::from(e)),
        };
        let claim_kind = parse_claim_kind(&self.claim_kind);

        let request = CompletionRequest {
            repo,
            claim,
            claim_kind,
            timeout_ms: timeout,
        };
        let report = match run_completion_check(&request) {
            Ok(r) => r,
            Err(e) => return Ok(CallToolResult::from(e)),
        };

        let json = serde_json::to_string_pretty(&report)
            .map_err(|e| CallToolError::from_message(format!("serialization error: {e}")))?;
        Ok(CallToolResult::text_content(vec![TextContent::from(json)]))
    }
}

// ---------------------------------------------------------------------------
// self_grill tool (M7 full implementation)
// ---------------------------------------------------------------------------
// self_grill tool (M7 full implementation)
// ---------------------------------------------------------------------------

#[macros::mcp_tool(
    name = "griller.self_grill",
    description = "M7 deterministic self-audit of the Griller's own decision pipeline. Produces an immutable original decision + a self-grill result with 16 structural checks. Never overwrites the original verdict."
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SelfGrillTool {
    pub claim: String,
    pub repo: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claim_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_dir: Option<String>,
}

impl SelfGrillTool {
    pub fn call_tool(&self) -> Result<CallToolResult, CallToolError> {
        let claim = match validate_text(&self.claim, "claim") {
            Ok(c) => c,
            Err(e) => return Ok(CallToolResult::from(e)),
        };
        let repo = match validate_repo_path(&self.repo) {
            Ok(r) => r,
            Err(e) => return Ok(CallToolResult::from(e)),
        };
        let claim_kind = parse_claim_kind(&self.claim_kind);
        let evidence_dir = self.evidence_dir.as_deref();

        let input = ClaimInput::new(&claim, ClaimSource::Agent).with_kind(claim_kind);
        let adjudicator = Adjudicator::deterministic();

        let normalized = match adjudicator.normalize(&input) {
            Ok(nc) => nc,
            Err(e) => {
                return Ok(error_result(
                    McpErrorKind::InternalError,
                    &format!("normalization failed: {e}"),
                ));
            }
        };

        let evidence = match load_evidence(&normalized, repo.as_path(), evidence_dir) {
            Ok(e) => e,
            Err(e) => {
                return Ok(error_result(
                    McpErrorKind::ExecutionFailed,
                    &format!("evidence load failed: {e}"),
                ));
            }
        };

        let assessment = match adjudicator.assess(&input, &evidence, None) {
            Ok(a) => a,
            Err(e) => {
                return Ok(error_result(
                    McpErrorKind::InternalError,
                    &format!("adjudication failed: {e}"),
                ));
            }
        };

        let severity = match assessment.status {
            EpistemicStatus::Verified | EpistemicStatus::AdversariallyVerified => Severity::Info,
            EpistemicStatus::Unsupported | EpistemicStatus::Unverifiable => Severity::High,
            EpistemicStatus::Uncertain => Severity::Medium,
            EpistemicStatus::Contradicted | EpistemicStatus::False => Severity::High,
            _ => Severity::Medium,
        };

        let policy_input = PolicyInput {
            assessment: assessment.clone(),
            context: PolicyContext::new(
                severity,
                griller_policy::severity::SeveritySource::Declared,
            ),
        };
        let decision = PolicyTable.decide(&assessment, &policy_input.context);

        let sg_input = griller_self_grill::SelfGrillInput::from_pipeline(
            &assessment,
            &decision,
            None,
            None,
            &evidence,
            severity,
            griller_policy::severity::SeveritySource::Declared,
        );

        let (original, result) = griller_self_grill::self_grill(&sg_input);

        let response = SelfGrillResponse {
            original,
            self_grill: result,
        };

        let json = serde_json::to_string_pretty(&response)
            .map_err(|e| CallToolError::from_message(format!("serialization error: {e}")))?;
        Ok(CallToolResult::text_content(vec![TextContent::from(json)]))
    }
}

fn load_evidence(
    _normalized: &griller_adjudicate::NormalizedClaim,
    repo: &std::path::Path,
    evidence_dir: Option<&str>,
) -> Result<Vec<Evidence>, String> {
    if let Some(dir) = evidence_dir {
        use griller_runtime::store::EvidenceStore;
        let store = griller_runtime::store::JsonStore::new(dir)
            .map_err(|e| format!("evidence store open failed: {e}"))?;
        return store
            .load_all()
            .map_err(|e| format!("evidence store load failed: {e}"));
    }
    // Minimal evidence: repository state
    let env = griller_runtime::execution::EnvSource::from_host();
    let collector = griller_runtime::collectors::RepositoryStateCollector::new(
        griller_runtime::collectors::CollectorEnvironment { env: env.clone() },
    );
    match collector.collect_context(repo) {
        Ok(_) => Ok(Vec::new()),
        Err(e) => Err(format!("context collection failed: {e}")),
    }
}

#[derive(Debug, Serialize)]
pub struct SelfGrillResponse {
    pub original: griller_self_grill::OriginalDecision,
    pub self_grill: griller_self_grill::SelfGrillResult,
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

fn parse_claim_kind(raw: &Option<String>) -> ClaimKind {
    match raw.as_deref() {
        Some("completion") => ClaimKind::Completion,
        Some("fact") => ClaimKind::Fact,
        Some("causal") => ClaimKind::Causal,
        Some("requirement") => ClaimKind::Requirement,
        Some("design") => ClaimKind::Design,
        _ => ClaimKind::Completion,
    }
}

fn parse_target(raw: &Option<String>) -> ClaimTarget {
    match raw {
        Some(t) if t.is_empty() => ClaimTarget::repository(),
        Some(t) => ClaimTarget::new(TargetScope::Unknown, Some(t.to_string())),
        None => ClaimTarget::repository(),
    }
}

fn parse_severity(raw: &Option<String>) -> Option<Severity> {
    match raw.as_deref() {
        Some("info") => Some(Severity::Info),
        Some("low") => Some(Severity::Low),
        Some("medium") => Some(Severity::Medium),
        Some("high") => Some(Severity::High),
        Some("critical") => Some(Severity::Critical),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Tool box and server handler
// ---------------------------------------------------------------------------

rust_mcp_sdk::tool_box!(
    GrillerTools,
    [
        HealthTool,
        VerifyClaimTool,
        CheckContextTool,
        RunAttacksTool,
        CheckCompletionTool,
        SelfGrillTool
    ]
);

pub struct GrillerMcpHandler;

impl Default for GrillerMcpHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl GrillerMcpHandler {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ServerHandler for GrillerMcpHandler {
    async fn handle_list_tools_request(
        &self,
        _params: Option<PaginatedRequestParams>,
        _runtime: Arc<dyn McpServer>,
    ) -> std::result::Result<ListToolsResult, RpcError> {
        Ok(ListToolsResult {
            tools: GrillerTools::tools(),
            meta: None,
            next_cursor: None,
        })
    }

    async fn handle_call_tool_request(
        &self,
        params: rust_mcp_sdk::schema::CallToolRequestParams,
        _runtime: Arc<dyn McpServer>,
    ) -> std::result::Result<CallToolResult, CallToolError> {
        let tool: GrillerTools = GrillerTools::try_from(params).map_err(CallToolError::new)?;
        match tool {
            GrillerTools::HealthTool(t) => t.call_tool(),
            GrillerTools::VerifyClaimTool(t) => t.call_tool(),
            GrillerTools::CheckContextTool(t) => t.call_tool(),
            GrillerTools::RunAttacksTool(t) => t.call_tool(),
            GrillerTools::CheckCompletionTool(t) => t.call_tool(),
            GrillerTools::SelfGrillTool(t) => t.call_tool(),
        }
    }

    async fn handle_list_resources_request(
        &self,
        _params: Option<PaginatedRequestParams>,
        _runtime: Arc<dyn McpServer>,
    ) -> std::result::Result<ListResourcesResult, RpcError> {
        Ok(ListResourcesResult {
            meta: None,
            next_cursor: None,
            resources: vec![],
        })
    }
}

// ---------------------------------------------------------------------------
// Server entry point
// ---------------------------------------------------------------------------

pub async fn run_server() -> SdkResult<()> {
    let server_details = InitializeResult {
        server_info: Implementation {
            name: "the-griller-mcp".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            title: Some("The Griller MCP Server".into()),
            description: Some("MCP adapter for The Griller: claim verification, context checking, adversarial attacks, and completion evaluation.".into()),
            icons: vec![],
            website_url: None,
        },
        capabilities: rust_mcp_sdk::schema::ServerCapabilities {
            tools: Some(rust_mcp_sdk::schema::ServerCapabilitiesTools { list_changed: None }),
            ..Default::default()
        },
        protocol_version: rust_mcp_sdk::schema::ProtocolVersion::V2025_11_25.into(),
        instructions: None,
        meta: None,
    };

    let transport = rust_mcp_sdk::StdioTransport::new(rust_mcp_sdk::TransportOptions::default())?;
    let handler = GrillerMcpHandler::new().to_mcp_server_handler();
    let server = server_runtime::create_server(McpServerOptions {
        transport,
        handler,
        server_details,
        task_store: None,
        client_task_store: None,
        message_observer: None,
    });
    server.start().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_returns_valid_json() {
        let tool = HealthTool::default();
        let result = tool.call_tool().unwrap();
        assert!(!result.is_error.unwrap_or(false));
        let text = result.content[0].as_text_content().unwrap().text.clone();
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed["service"], "the-griller");
        assert_eq!(parsed["capabilities"]["self_grill"], true);
        assert_eq!(parsed["execution"]["network_isolation"], false);
        assert_eq!(parsed["execution"]["os_sandbox"], false);
        assert_eq!(parsed["security"]["network_isolation"], "not_enforced");
        assert_eq!(parsed["security"]["os_sandbox"], "not_enforced");
    }

    #[test]
    fn verify_claim_rejects_empty_repo() {
        let tool = VerifyClaimTool {
            repo: "".to_string(),
            claim: "tests pass".to_string(),
            claim_kind: None,
            target: None,
            severity: None,
            evidence_mode: "minimal".to_string(),
        };
        let result = tool.call_tool().unwrap();
        assert!(result.is_error.unwrap_or(false));
        let text = result.content[0].as_text_content().unwrap().text.clone();
        let err: McpError = serde_json::from_str(&text).unwrap();
        assert_eq!(err.kind, McpErrorKind::InvalidRequest);
    }

    #[test]
    fn verify_claim_rejects_nonexistent_repo() {
        let tool = VerifyClaimTool {
            repo: "/nonexistent/path/xyz123".to_string(),
            claim: "tests pass".to_string(),
            claim_kind: None,
            target: None,
            severity: None,
            evidence_mode: "minimal".to_string(),
        };
        let result = tool.call_tool().unwrap();
        assert!(result.is_error.unwrap_or(false));
        let text = result.content[0].as_text_content().unwrap().text.clone();
        let err: McpError = serde_json::from_str(&text).unwrap();
        assert_eq!(err.kind, McpErrorKind::RepositoryNotFound);
    }

    #[test]
    fn verify_claim_rejects_empty_claim() {
        let tool = VerifyClaimTool {
            repo: "/tmp".to_string(),
            claim: "   ".to_string(),
            claim_kind: None,
            target: None,
            severity: None,
            evidence_mode: "minimal".to_string(),
        };
        let result = tool.call_tool().unwrap();
        assert!(result.is_error.unwrap_or(false));
    }

    #[test]
    fn check_context_rejects_empty_repo() {
        let tool = CheckContextTool {
            repo: "".to_string(),
            task: "implement feature x".to_string(),
        };
        let result = tool.call_tool().unwrap();
        assert!(result.is_error.unwrap_or(false));
    }

    #[test]
    fn check_context_rejects_empty_task() {
        let tool = CheckContextTool {
            repo: "/tmp".to_string(),
            task: "".to_string(),
        };
        let result = tool.call_tool().unwrap();
        assert!(result.is_error.unwrap_or(false));
    }

    #[test]
    fn run_attacks_rejects_empty_repo() {
        let tool = RunAttacksTool {
            repo: "".to_string(),
            claim: "tests pass".to_string(),
            claim_kind: None,
            target: None,
            attack_kinds: None,
            timeout: 0,
        };
        let result = tool.call_tool().unwrap();
        assert!(result.is_error.unwrap_or(false));
    }

    #[test]
    fn run_attacks_rejects_unsafe_command() {
        let spec = griller_core::attack::AttackSpec {
            id: griller_core::attack::AttackId::from_content_hash("unsafe"),
            kind: griller_core::attack::AttackKind::Replay,
            target_claim: "test".into(),
            command: Some(
                griller_core::command::CommandSpec::new("echo").with_args(["$(rm -rf /)"]),
            ),
            working_state: None,
            timeout_ms: 30_000,
            rationale: None,
        };
        let validation = griller_runtime::attack::AttackExecutor::validate(&spec);
        assert!(!validation.allowed);
    }

    #[test]
    fn self_grill_returns_limited_capability() {
        let tool = SelfGrillTool {
            claim: "all tests pass".to_string(),
            repo: "/tmp".to_string(),
            claim_kind: None,
            evidence_dir: None,
        };
        let result = tool.call_tool().unwrap();
        // /tmp is not a git repo, so we expect an execution error
        assert!(result.is_error.unwrap_or(false));
        let text = result.content[0].as_text_content().unwrap().text.clone();
        let err: McpError = serde_json::from_str(&text).unwrap();
        assert_eq!(err.kind, McpErrorKind::ExecutionFailed);
    }

    #[test]
    fn tool_names_are_correct() {
        let tools = GrillerTools::tools();
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"griller.health"));
        assert!(names.contains(&"griller.verify_claim"));
        assert!(names.contains(&"griller.check_context"));
        assert!(names.contains(&"griller.run_attacks"));
        assert!(names.contains(&"griller.check_completion"));
        assert!(names.contains(&"griller.self_grill"));
        assert_eq!(names.len(), 6);
    }

    #[test]
    fn check_context_rejects_oversized_task() {
        let tool = CheckContextTool {
            repo: "/tmp".to_string(),
            task: "x".repeat(MAX_TEXT_LEN + 1),
        };
        let result = tool.call_tool().unwrap();
        assert!(result.is_error.unwrap_or(false));
        let text = result.content[0].as_text_content().unwrap().text.clone();
        let err: McpError = serde_json::from_str(&text).unwrap();
        assert_eq!(err.kind, McpErrorKind::InvalidRequest);
    }

    #[test]
    fn check_completion_rejects_oversized_claim() {
        let tool = CheckCompletionTool {
            repo: "/tmp".to_string(),
            claim: "y".repeat(MAX_TEXT_LEN + 1),
            claim_kind: None,
            timeout: 0,
        };
        let result = tool.call_tool().unwrap();
        assert!(result.is_error.unwrap_or(false));
        let text = result.content[0].as_text_content().unwrap().text.clone();
        let err: McpError = serde_json::from_str(&text).unwrap();
        assert_eq!(err.kind, McpErrorKind::InvalidRequest);
    }

    #[test]
    fn run_attacks_rejects_oversized_attack_list() {
        let tool = RunAttacksTool {
            repo: "/tmp".to_string(),
            claim: "tests pass".to_string(),
            claim_kind: None,
            target: None,
            attack_kinds: Some((0..MAX_ATTACKS + 1).map(|i| format!("kind_{i}")).collect()),
            timeout: 0,
        };
        let result = tool.call_tool().unwrap();
        assert!(result.is_error.unwrap_or(false));
        let text = result.content[0].as_text_content().unwrap().text.clone();
        let err: McpError = serde_json::from_str(&text).unwrap();
        assert_eq!(err.kind, McpErrorKind::InvalidRequest);
    }

    #[test]
    fn run_attacks_rejects_unknown_kind() {
        let tool = RunAttacksTool {
            repo: "/tmp".to_string(),
            claim: "tests pass".to_string(),
            claim_kind: None,
            target: None,
            attack_kinds: Some(vec!["replay".into(), "mind_read".into()]),
            timeout: 0,
        };
        let result = tool.call_tool().unwrap();
        assert!(result.is_error.unwrap_or(false));
        let text = result.content[0].as_text_content().unwrap().text.clone();
        let err: McpError = serde_json::from_str(&text).unwrap();
        assert_eq!(err.kind, McpErrorKind::InvalidRequest);
        assert!(err.message.contains("mind_read"));
    }
}
