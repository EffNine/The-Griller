//! Shared in-memory project fixtures for griller-context integration tests.
//!
//! These mirror the static fixtures under `tests/fixtures` as collected by
//! `ProjectIdentityCollector`. They are pure data so the pure context crate
//! can be tested without any I/O.

#![allow(dead_code)]

use griller_context::project::ProjectIdentity;

/// CodeBro-like repository: deterministic engineering context + MCP tooling.
pub fn codebro_project() -> ProjectIdentity {
    ProjectIdentity {
        root_dir_name: "codebro".into(),
        project_names: vec!["CodeBro".into(), "codebro".into()],
        package_names: vec!["griller-core".into(), "griller-mcp".into()],
        repository_names: vec!["codebro".into()],
        remotes: vec!["https://github.com/owner/codebro.git".into()],
        languages: vec!["rust".into()],
        package_managers: vec!["cargo".into()],
        manifest_paths: vec![
            "Cargo.toml".into(),
            "crates/griller-core/Cargo.toml".into(),
            "crates/griller-mcp/Cargo.toml".into(),
        ],
        top_level_entries: vec!["crates".into(), "src".into()],
        important_paths: vec![
            "src/lib.rs".into(),
            "crates/griller-core/src/lib.rs".into(),
            "crates/griller-mcp/src/lib.rs".into(),
        ],
        all_paths: vec![
            "Cargo.toml".into(),
            "crates/griller-core/Cargo.toml".into(),
            "crates/griller-core/src/lib.rs".into(),
            "crates/griller-mcp/Cargo.toml".into(),
            "crates/griller-mcp/src/lib.rs".into(),
            "src/lib.rs".into(),
        ],
        symbols: vec!["claim".into(), "fact".into(), "grill".into()],
        readme_title: Some("CodeBro".into()),
        vocabulary: vec![
            "boundary".into(),
            "codebro".into(),
            "core".into(),
            "deterministic".into(),
            "engineering".into(),
            "fact".into(),
            "griller".into(),
            "mcp".into(),
            "package".into(),
            "workspace".into(),
        ],
    }
}

/// Atlas-like repository: a deterministic fact store.
pub fn atlas_project() -> ProjectIdentity {
    ProjectIdentity {
        root_dir_name: "atlas".into(),
        project_names: vec!["Atlas".into(), "Atlas Fact Store".into()],
        package_names: vec!["atlas".into()],
        repository_names: vec!["atlas".into()],
        remotes: vec!["https://github.com/owner/atlas.git".into()],
        languages: vec!["rust".into()],
        package_managers: vec!["cargo".into()],
        manifest_paths: vec!["Cargo.toml".into()],
        top_level_entries: vec!["src".into()],
        important_paths: vec!["Cargo.toml".into(), "src/lib.rs".into()],
        all_paths: vec![
            "Cargo.toml".into(),
            "src/fact_store.rs".into(),
            "src/lib.rs".into(),
        ],
        symbols: vec!["factstore".into(), "ingest".into(), "query".into()],
        readme_title: Some("Atlas Fact Store".into()),
        vocabulary: vec![
            "atlas".into(),
            "fact".into(),
            "facts".into(),
            "store".into(),
            "confidence".into(),
            "provenance".into(),
        ],
    }
}

/// Meridian-like repository: control plane with a weak Atlas adapter crate.
pub fn meridian_project() -> ProjectIdentity {
    ProjectIdentity {
        root_dir_name: "meridian".into(),
        project_names: vec!["Meridian".into(), "meridian".into()],
        package_names: vec!["atlas-adapter".into()],
        repository_names: vec!["meridian".into()],
        remotes: vec!["https://github.com/owner/meridian.git".into()],
        languages: vec!["rust".into()],
        package_managers: vec!["cargo".into()],
        manifest_paths: vec![
            "Cargo.toml".into(),
            "crates/atlas-adapter/Cargo.toml".into(),
        ],
        top_level_entries: vec!["crates".into()],
        important_paths: vec![
            "Cargo.toml".into(),
            "crates/atlas-adapter/src/lib.rs".into(),
        ],
        all_paths: vec![
            "Cargo.toml".into(),
            "crates/atlas-adapter/Cargo.toml".into(),
            "crates/atlas-adapter/src/lib.rs".into(),
        ],
        symbols: vec!["read_fact".into()],
        readme_title: Some("Meridian".into()),
        vocabulary: vec![
            "adapter".into(),
            "atlas".into(),
            "deployment".into(),
            "legacy".into(),
            "meridian".into(),
            "routing".into(),
            "telemetry".into(),
        ],
    }
}

/// A generic application with no strong identity signals.
pub fn generic_project() -> ProjectIdentity {
    ProjectIdentity {
        root_dir_name: "shop-app".into(),
        project_names: vec!["shop-app".into(), "Shop App".into()],
        package_names: vec!["shop-app".into()],
        repository_names: vec!["shop-app".into()],
        remotes: vec!["https://github.com/owner/shop-app.git".into()],
        languages: vec!["javascript".into()],
        package_managers: vec!["npm".into()],
        manifest_paths: vec!["package.json".into()],
        top_level_entries: vec!["src".into()],
        important_paths: vec!["src/index.js".into()],
        all_paths: vec!["package.json".into(), "src/index.js".into()],
        symbols: vec!["express".into(), "module".into()],
        readme_title: Some("Shop App".into()),
        vocabulary: vec![
            "app".into(),
            "catalog".into(),
            "checkout".into(),
            "customers".into(),
            "products".into(),
            "shop".into(),
        ],
    }
}
