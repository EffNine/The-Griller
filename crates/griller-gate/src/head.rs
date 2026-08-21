//! Shell-free HEAD commit resolution from `.git` bookkeeping files.
//!
//! Reads `.git/HEAD` (and the referenced ref file, or the packed-refs file)
//! directly. Never invokes git or any other process — the repository is
//! untrusted input, and parsing is deliberately best-effort.

use std::fs;
use std::path::Path;

/// Resolve the current HEAD commit of a repository without running any
/// process. Returns `None` when the repository has no commit or the
/// bookkeeping is in an unsupported layout.
pub fn resolve_head_commit(repo: &Path) -> Option<String> {
    let git_dir = find_git_dir(repo)?;
    let head_path = git_dir.join("HEAD");
    let head = fs::read_to_string(head_path).ok()?;
    let head = head.trim();

    if let Some(ref_path) = head.strip_prefix("ref: ") {
        let ref_name = ref_path.trim();
        if let Ok(content) = fs::read_to_string(git_dir.join(ref_name)) {
            let sha = content.trim();
            if is_sha(sha) {
                return Some(sha.to_string());
            }
        }
        // Fall back to packed-refs for refs that are not loose files.
        let packed = fs::read_to_string(git_dir.join("packed-refs")).ok()?;
        packed.lines().find_map(|line| {
            let mut parts = line.split_whitespace();
            let sha = parts.next()?;
            let name = parts.next()?;
            if name == ref_name && is_sha(sha) {
                Some(sha.to_string())
            } else {
                None
            }
        })
    } else if is_sha(head) {
        // Detached HEAD.
        Some(head.to_string())
    } else {
        None
    }
}

fn find_git_dir(repo: &Path) -> Option<std::path::PathBuf> {
    let direct = repo.join(".git");
    if direct.is_dir() {
        return Some(direct);
    }
    // Worktrees / submodules use a `.git` *file* pointing at the real dir.
    let git_file = fs::read_to_string(&direct).ok()?;
    let line = git_file.trim().strip_prefix("gitdir: ")?;
    let resolved = if Path::new(line).is_absolute() {
        std::path::PathBuf::from(line)
    } else {
        repo.join(line)
    };
    resolved.is_dir().then_some(resolved)
}

fn is_sha(s: &str) -> bool {
    s.len() == 40 && s.chars().all(|c| c.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init_repo(dir: &std::path::Path) -> std::path::PathBuf {
        let git = dir.join(".git");
        fs::create_dir_all(git.join("refs/heads")).unwrap();
        git
    }

    #[test]
    fn resolves_loose_ref_head() {
        let dir = tempfile::tempdir().unwrap();
        let git = init_repo(dir.path());
        fs::write(git.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        fs::write(
            git.join("refs/heads/main"),
            "0123456789abcdef0123456789abcdef01234567\n",
        )
        .unwrap();
        assert_eq!(
            resolve_head_commit(dir.path()).as_deref(),
            Some("0123456789abcdef0123456789abcdef01234567")
        );
    }

    #[test]
    fn resolves_detached_head() {
        let dir = tempfile::tempdir().unwrap();
        let git = init_repo(dir.path());
        fs::write(
            git.join("HEAD"),
            "0123456789abcdef0123456789abcdef01234567\n",
        )
        .unwrap();
        assert_eq!(
            resolve_head_commit(dir.path()).as_deref(),
            Some("0123456789abcdef0123456789abcdef01234567")
        );
    }

    #[test]
    fn resolves_packed_refs_fallback() {
        let dir = tempfile::tempdir().unwrap();
        let git = init_repo(dir.path());
        fs::write(git.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        fs::write(
            git.join("packed-refs"),
            "# pack-refs with: peeled fully-peeled sorted\n\
             0123456789abcdef0123456789abcdef01234567 refs/heads/main\n",
        )
        .unwrap();
        assert_eq!(
            resolve_head_commit(dir.path()).as_deref(),
            Some("0123456789abcdef0123456789abcdef01234567")
        );
    }

    #[test]
    fn empty_repo_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let git = init_repo(dir.path());
        fs::write(git.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        assert_eq!(resolve_head_commit(dir.path()), None);
    }

    #[test]
    fn missing_git_dir_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(resolve_head_commit(dir.path()), None);
    }

    #[test]
    fn worktree_git_file_resolves() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real-git");
        fs::create_dir_all(real.join("refs/heads")).unwrap();
        fs::write(real.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        fs::write(
            real.join("refs/heads/main"),
            "0123456789abcdef0123456789abcdef01234567\n",
        )
        .unwrap();
        fs::write(
            dir.path().join(".git"),
            format!("gitdir: {}", real.display()),
        )
        .unwrap();
        assert_eq!(
            resolve_head_commit(dir.path()).as_deref(),
            Some("0123456789abcdef0123456789abcdef01234567")
        );
    }
}
