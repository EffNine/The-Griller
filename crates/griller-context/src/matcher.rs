//! Deterministic context matching.
//!
//! The matcher turns a normalized [`TaskIdentity`] and a
//! [`ProjectIdentity`](crate::project::ProjectIdentity) into structured
//! [`ContextEvidence`]. It is the **relevance decider**: the LLM may propose
//! candidate entities, but only this deterministic layer decides what they
//! mean against the repository.
//!
//! ## Matching rules (documented)
//!
//! 1. Exact manifest/project-name match -> strong positive
//! 2. Git remote repository-name match -> strong positive
//! 3. Explicit project-name match -> strong positive
//! 4. Referenced file/path exists -> strong positive
//! 5. Referenced module/symbol exists -> medium positive
//! 6. Relevant manifest/package exists -> medium positive
//! 7. Feature/domain term presence -> weak positive
//! 8. Technology compatibility -> weak positive
//! 9. Explicit contradictory project identity -> strong negative (only when
//!    the foreign entity has **no** supporting presence in the repository)
//!
//! Absence is classified conservatively:
//!
//! * explicit entity absent + explicit project mismatch -> strong negative
//! * explicit entity absent alone -> weak negative
//! * entity absent + generic task -> no useful context signal

use crate::evidence::{ContextEvidence, ContextEvidenceKind as Kind, EvidenceStrength as Strength};
use crate::name::{normalize_name, normalize_path};
use crate::project::ProjectIdentity;
use crate::task::{EntityKind, TaskEntity, TaskIdentity};

/// The context matching boundary.
///
/// Implementations are deterministic: the same (task, project) always
/// produce the same evidence.
pub trait ContextMatcher: Send + Sync {
    /// Produce structured context evidence for a task against a project.
    fn match_context(&self, task: &TaskIdentity, project: &ProjectIdentity)
        -> Vec<ContextEvidence>;
}

/// The default deterministic matcher implementing the documented rules.
#[derive(Debug, Clone, Copy, Default)]
pub struct DeterministicContextMatcher;

impl ContextMatcher for DeterministicContextMatcher {
    fn match_context(
        &self,
        task: &TaskIdentity,
        project: &ProjectIdentity,
    ) -> Vec<ContextEvidence> {
        let mut evidence: Vec<ContextEvidence> = Vec::new();

        // Rule 3: explicit project-name match / rule 9: explicit contradictory
        // project identity, plus presence-conflict handling.
        for entity in task.entities_of(EntityKind::Project) {
            match_project_entity(project, entity, &mut evidence);
        }

        // Rule 4: referenced file/path exists.
        for entity in task.entities_of(EntityKind::Path) {
            match_path_entity(project, entity, &mut evidence);
        }

        // Rule 5: referenced module exists.
        for entity in task.entities_of(EntityKind::Module) {
            match_module_entity(project, entity, &mut evidence);
        }

        // Rule 5: referenced symbol exists.
        for entity in task.entities_of(EntityKind::Symbol) {
            match_symbol_entity(project, entity, &mut evidence);
        }

        // Rule 1/2/3 via content tokens: catch lowercase references to the
        // repository (e.g. "fix codebro bug") and rules 6-8 for terminology
        // and technology overlap.
        match_content_tokens(task, project, &mut evidence);

        dedupe_and_sort(&mut evidence);
        evidence
    }
}

/// Match a project-name candidate entity against the repository.
fn match_project_entity(
    project: &ProjectIdentity,
    entity: &TaskEntity,
    evidence: &mut Vec<ContextEvidence>,
) {
    // Rule 3: explicit project-name match.
    if project.matches_identity(&entity.normalized) {
        evidence.push(
            ContextEvidence::positive(
                Kind::ExplicitProjectMatch,
                &entity.name,
                Strength::Strong,
                "The task explicitly names this repository's project.",
            )
            .with_refs([&entity.name])
            .with_repository_identity(project.primary_identity()),
        );
        return;
    }

    // Distinguish a foreign project name with no presence from one that has
    // weak supporting presence (a structured conflict).
    let presence: Vec<String> = project
        .presence_terms()
        .into_iter()
        .filter(|term| term == &entity.normalized)
        .collect();

    if presence.is_empty() {
        // Rule 9: explicit contradictory project identity.
        evidence.push(
            ContextEvidence::negative(
                Kind::ExplicitProjectMismatch,
                &entity.name,
                Strength::Strong,
                format!(
                    "The task explicitly names \"{}\" and the repository has no presence of it.",
                    entity.name
                ),
            )
            .with_refs(["no matching module, path, symbol or term"])
            .with_repository_identity(project.primary_identity()),
        );
    } else {
        evidence.push(
            ContextEvidence::negative(
                Kind::ConflictingEntityReference,
                &entity.name,
                Strength::Weak,
                format!(
                    "The task names \"{}\" which is not this repository's identity but has weak presence here.",
                    entity.name
                ),
            )
            .with_refs(presence)
            .with_repository_identity(project.primary_identity()),
        );
        evidence.push(ContextEvidence::positive(
            Kind::EntityPresentInRepository,
            &entity.name,
            Strength::Weak,
            "The named entity has weak presence in this repository.",
        ));
    }
}

/// Match a referenced path against the repository's path index.
fn match_path_entity(
    project: &ProjectIdentity,
    entity: &TaskEntity,
    evidence: &mut Vec<ContextEvidence>,
) {
    let normalized = normalize_path(&entity.name);
    if let Some(found) = project.path_exists(&normalized) {
        evidence.push(
            ContextEvidence::positive(
                Kind::ReferencedPathExists,
                &entity.name,
                Strength::Strong,
                "The referenced file or path exists in the repository.",
            )
            .with_refs([found]),
        );
        return;
    }
    // A basename match ("fact_store.rs" -> "src/fact_store.rs").
    let stem = normalize_name(&entity.name);
    if let Some(found) = project
        .all_paths
        .iter()
        .find(|p| normalize_name(p) == stem || p.ends_with(&entity.name))
    {
        evidence.push(
            ContextEvidence::positive(
                Kind::ReferencedPathExists,
                &entity.name,
                Strength::Medium,
                "The referenced file exists under a different path.",
            )
            .with_refs([found.clone()]),
        );
        return;
    }
    evidence.push(
        ContextEvidence::negative(
            Kind::ReferencedEntityAbsent,
            &entity.name,
            Strength::Weak,
            "The referenced path is absent from the repository.",
        )
        .with_refs([normalized]),
    );
}

/// Match a referenced module against repository paths and entries.
fn match_module_entity(
    project: &ProjectIdentity,
    entity: &TaskEntity,
    evidence: &mut Vec<ContextEvidence>,
) {
    let normalized = normalize_name(&entity.normalized);
    let hit = project.important_paths.iter().find(|p| {
        normalize_name(p) == normalized
            || normalize_name(&crate::name::basename_stem(p)) == normalized
            || p.split('/')
                .any(|component| normalize_name(component) == normalized)
    });
    let dir_hit = project
        .top_level_entries
        .iter()
        .find(|e| normalize_name(e) == normalized);
    let path_hit = project.all_paths.iter().find(|p| {
        p.split('/')
            .any(|component| normalize_name(component) == normalized)
    });
    if let Some(path) = hit.or(dir_hit).or(path_hit) {
        evidence.push(
            ContextEvidence::positive(
                Kind::ReferencedModuleExists,
                &entity.name,
                Strength::Medium,
                "The referenced module or directory exists in the repository.",
            )
            .with_refs([path.clone()]),
        );
    } else {
        evidence.push(
            ContextEvidence::negative(
                Kind::ReferencedEntityAbsent,
                &entity.name,
                Strength::Weak,
                "The referenced module is absent from the repository.",
            )
            .with_refs([entity.normalized.clone()]),
        );
    }
}

/// Match a referenced symbol against the repository's symbol index.
fn match_symbol_entity(
    project: &ProjectIdentity,
    entity: &TaskEntity,
    evidence: &mut Vec<ContextEvidence>,
) {
    let normalized = normalize_name(&entity.normalized);
    if project.symbols.iter().any(|s| s == &normalized) {
        evidence.push(
            ContextEvidence::positive(
                Kind::ReferencedSymbolExists,
                &entity.name,
                Strength::Medium,
                "The referenced symbol exists in the repository's source.",
            )
            .with_refs([normalized]),
        );
    } else {
        evidence.push(
            ContextEvidence::negative(
                Kind::ReferencedEntityAbsent,
                &entity.name,
                Strength::Weak,
                "The referenced symbol is absent from the repository.",
            )
            .with_refs([normalized]),
        );
    }
}

/// Match content tokens against identity names, modules, symbols, vocabulary
/// and technology.
///
/// Rules 1/2/3 are also reachable here for lowercase references the
/// normalizer did not classify as explicit project entities ("fix codebro
/// bug"). Rules 5, 6, 7 and 8 are applied from module/path components,
/// symbols, vocabulary and technology terms.
fn match_content_tokens(
    task: &TaskIdentity,
    project: &ProjectIdentity,
    evidence: &mut Vec<ContextEvidence>,
) {
    let identity_names = project.identity_names();
    let vocabulary = project.vocabulary_terms();
    let technologies = project.technology_terms();
    let symbols = project.symbols.clone();

    let mut domain_hits: Vec<String> = Vec::new();
    let mut module_hits: Vec<String> = Vec::new();
    let mut symbol_hits: Vec<String> = Vec::new();

    for token in &task.content_tokens {
        if identity_names.contains(token) {
            let kind = if project
                .package_names
                .iter()
                .any(|n| normalize_name(n) == *token)
            {
                Kind::ManifestMatch
            } else if project
                .repository_names
                .iter()
                .any(|n| normalize_name(n) == *token)
            {
                Kind::RemoteMatch
            } else {
                Kind::ExplicitProjectMatch
            };
            evidence.push(
                ContextEvidence::positive(
                    kind,
                    token,
                    Strength::Strong,
                    "A task token matches a repository identity name.",
                )
                .with_refs([token.clone()])
                .with_repository_identity(project.primary_identity()),
            );
            continue;
        }
        if technologies.contains(token) {
            evidence.push(
                ContextEvidence::positive(
                    Kind::TechnologyCompatibility,
                    token,
                    Strength::Weak,
                    "The task references a technology this repository uses.",
                )
                .with_refs([token.clone()]),
            );
            continue;
        }
        if symbols.contains(token) {
            symbol_hits.push(token.clone());
            continue;
        }
        // A token that matches a path component, top-level entry, or an
        // important-path stem is a module reference.
        if module_matches(project, token) {
            module_hits.push(token.clone());
            continue;
        }
        if vocabulary.contains(token) {
            domain_hits.push(token.clone());
        }
    }

    for token in symbol_hits {
        evidence.push(
            ContextEvidence::positive(
                Kind::ReferencedSymbolExists,
                &token,
                Strength::Medium,
                "A task token matches a symbol in the repository's source.",
            )
            .with_refs([token.clone()]),
        );
    }

    for token in module_hits {
        evidence.push(
            ContextEvidence::positive(
                Kind::ReferencedModuleExists,
                &token,
                Strength::Medium,
                "A task token matches a module or path component in the repository.",
            )
            .with_refs([token.clone()]),
        );
    }

    if !domain_hits.is_empty() {
        domain_hits.sort();
        domain_hits.dedup();
        let strength = if domain_hits.len() >= 3 {
            Strength::Medium
        } else {
            Strength::Weak
        };
        evidence.push(
            ContextEvidence::positive(
                Kind::DomainTermMatch,
                "domain terminology",
                strength,
                format!(
                    "{} task term(s) overlap the repository vocabulary.",
                    domain_hits.len()
                ),
            )
            .with_refs(domain_hits),
        );
    }
}

/// True when a normalized token matches a repository module signal: a path
/// component, a top-level entry, an important-path stem, or a manifest path.
fn module_matches(project: &ProjectIdentity, token: &str) -> bool {
    let path_matches = |p: &String| {
        p.split('/')
            .any(|component| normalize_name(component) == token)
            || normalize_name(p) == token
    };
    project.important_paths.iter().any(path_matches)
        || project.all_paths.iter().any(path_matches)
        || project
            .top_level_entries
            .iter()
            .any(|e| normalize_name(e) == token)
        || project
            .manifest_paths
            .iter()
            .any(|p| normalize_name(&crate::name::basename_stem(p)) == token)
}

/// Deterministic ordering + deduplication of evidence.
fn dedupe_and_sort(evidence: &mut Vec<ContextEvidence>) {
    evidence.sort_by(|a, b| {
        (a.direction, a.kind, &a.subject, &a.explanation).cmp(&(
            b.direction,
            b.kind,
            &b.subject,
            &b.explanation,
        ))
    });
    evidence.dedup();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evidence::EvidenceDirection;
    use crate::normalizer::normalize_deterministic;
    use crate::project::ProjectIdentity;

    fn codebro_project() -> ProjectIdentity {
        ProjectIdentity {
            root_dir_name: "codebro".into(),
            project_names: vec!["CodeBro".into()],
            package_names: vec!["griller-core".into(), "griller-mcp".into()],
            repository_names: vec!["codebro".into()],
            remotes: vec!["https://github.com/owner/codebro.git".into()],
            languages: vec!["rust".into()],
            package_managers: vec!["cargo".into()],
            manifest_paths: vec!["Cargo.toml".into()],
            top_level_entries: vec!["crates".into(), "src".into()],
            important_paths: vec![
                "src/lib.rs".into(),
                "crates/griller-core/src/lib.rs".into(),
                "crates/griller-mcp/src/lib.rs".into(),
            ],
            all_paths: vec![
                "Cargo.toml".into(),
                "crates/griller-core/src/lib.rs".into(),
                "crates/griller-mcp/src/lib.rs".into(),
                "src/lib.rs".into(),
            ],
            symbols: vec!["grill".into(), "mcp".into(), "claim".into()],
            readme_title: Some("CodeBro".into()),
            vocabulary: vec!["claim".into(), "evidence".into(), "mcp".into()],
        }
    }

    fn match_task(text: &str, project: &ProjectIdentity) -> Vec<ContextEvidence> {
        let task = normalize_deterministic(text);
        DeterministicContextMatcher.match_context(&task, project)
    }

    fn has_kind(evidence: &[ContextEvidence], kind: Kind) -> bool {
        evidence.iter().any(|e| e.kind == kind)
    }

    #[test]
    fn atlas_task_on_codebro_produces_mismatch_receipt() {
        let evidence = match_task("Implement Atlas P10.5.1 Fact Store.", &codebro_project());
        let mismatch = evidence
            .iter()
            .find(|e| e.kind == Kind::ExplicitProjectMismatch)
            .expect("explicit mismatch expected");
        assert_eq!(mismatch.subject, "Atlas");
        assert_eq!(mismatch.repository_identity.as_deref(), Some("CodeBro"));
        assert_eq!(mismatch.strength, Strength::Strong);
        assert_eq!(mismatch.direction, EvidenceDirection::Negative);
    }

    #[test]
    fn codebro_task_on_codebro_produces_identity_match() {
        let evidence = match_task("Fix CodeBro MCP tool registration.", &codebro_project());
        assert!(has_kind(&evidence, Kind::ExplicitProjectMatch));
        assert!(!has_kind(&evidence, Kind::ExplicitProjectMismatch));
    }

    #[test]
    fn lowercase_identity_reference_matches() {
        let evidence = match_task("fix codebro bug in claim assess", &codebro_project());
        // "codebro" matches identity via the git remote name (RemoteMatch) or
        // project name (ExplicitProjectMatch) — either way a strong identity
        // match.
        assert!(evidence.iter().any(|e| {
            matches!(
                e.kind,
                Kind::ExplicitProjectMatch | Kind::RemoteMatch | Kind::ManifestMatch
            ) && e.strength == Strength::Strong
        }));
    }

    #[test]
    fn remote_repo_name_matches() {
        let evidence = match_task("implement codebro remote sync", &codebro_project());
        assert!(has_kind(&evidence, Kind::RemoteMatch));
    }

    #[test]
    fn manifest_package_name_matches() {
        let evidence = match_task("fix griller-core", &codebro_project());
        assert!(has_kind(&evidence, Kind::ManifestMatch));
    }

    #[test]
    fn referenced_path_existing_is_positive() {
        let evidence = match_task("Fix src/lib.rs build", &codebro_project());
        assert!(has_kind(&evidence, Kind::ReferencedPathExists));
    }

    #[test]
    fn referenced_path_absent_is_weak_negative() {
        let evidence = match_task("Fix src/nope.rs", &codebro_project());
        assert!(has_kind(&evidence, Kind::ReferencedEntityAbsent));
    }

    #[test]
    fn module_and_symbol_presence() {
        let evidence = match_task(
            "Fix the griller-core module and the claim function",
            &codebro_project(),
        );
        assert!(has_kind(&evidence, Kind::ReferencedModuleExists));
        assert!(has_kind(&evidence, Kind::ReferencedSymbolExists));
    }

    #[test]
    fn technology_and_vocabulary_overlap() {
        let evidence = match_task("refactor the cargo evidence handling", &codebro_project());
        assert!(has_kind(&evidence, Kind::TechnologyCompatibility));
        assert!(has_kind(&evidence, Kind::DomainTermMatch));
    }

    #[test]
    fn foreign_entity_with_presence_is_conflict_not_mismatch() {
        // "Atlas" is a foreign project, but a legacy atlas file exists here.
        let mut project = codebro_project();
        project.all_paths.push("legacy/atlas.rs".into());
        project.all_paths.sort();
        project.symbols.push("atlas".into());
        project.symbols.sort();
        let evidence = match_task("Implement Atlas P10.5.1 Fact Store.", &project);
        assert!(!has_kind(&evidence, Kind::ExplicitProjectMismatch));
        assert!(has_kind(&evidence, Kind::ConflictingEntityReference));
    }
}
