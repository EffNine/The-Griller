//! Git interaction helpers built on top of the execution backend.
//!
//! Git is invoked through [`ExecutionBackend`] with an explicit allow-listed
//! environment and no shell, exactly like any other command. Git output is
//! treated as untrusted data.

use std::path::Path;

use griller_core::command::{CommandSpec, EnvironmentPolicy, NetworkPolicy};

use crate::execution::default_env_allowlist;

/// Build a [`CommandSpec`] for a git invocation in `dir`.
///
/// Network is disabled and the environment is allow-listed.
pub fn git_command(dir: &Path, args: &[&str]) -> CommandSpec {
    CommandSpec::new("git")
        .with_args(args.iter().map(|s| s.to_string()))
        .with_working_directory(dir)
        .with_environment_policy(EnvironmentPolicy::InheritAllowlist(default_env_allowlist()))
        .with_network_policy(NetworkPolicy::Disabled)
}

/// The git version string, e.g. `git 2.53.0`, if git is available.
pub fn git_version(env: &crate::execution::EnvSource) -> Option<String> {
    let spec = CommandSpec::new("git").with_args(["--version"]);
    let result = crate::execution::ExecutionBackend::execute(&spec, env).ok()?;
    result
        .succeeded()
        .then(|| result.stdout_lossy().trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution::EnvSource;

    #[test]
    fn git_command_is_shell_free_and_allowlisted() {
        let spec = git_command(Path::new("/tmp"), &["status", "--porcelain"]);
        assert_eq!(spec.executable, "git");
        assert_eq!(spec.args, ["status", "--porcelain"]);
        assert!(matches!(
            spec.environment_policy,
            EnvironmentPolicy::InheritAllowlist(_)
        ));
        assert_eq!(spec.network_policy, NetworkPolicy::Disabled);
    }

    #[test]
    fn git_version_detects_git() {
        let version = git_version(&EnvSource::from_host());
        assert!(
            version.is_some(),
            "git must be installed for griller to work"
        );
    }
}
