#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

struct GitFixture {
    _temp: TempDir,
    work: PathBuf,
    remote: PathBuf,
}

impl GitFixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("create temporary test directory");
        let work = temp.path().join("work");
        let remote = temp.path().join("remote.git");
        fs::create_dir(&work).expect("create work repository directory");
        git(
            &work,
            &[
                "init",
                "--bare",
                remote.to_str().expect("UTF-8 remote path"),
            ],
        );
        git(&work, &["init", "-b", "main"]);
        git(&work, &["config", "user.name", "GitGuard Test"]);
        git(&work, &["config", "user.email", "gitguard@example.invalid"]);
        git(
            &work,
            &[
                "remote",
                "add",
                "origin",
                remote.to_str().expect("UTF-8 path"),
            ],
        );
        fs::write(work.join("README.md"), "temporary test repository\n").expect("write test file");
        git(&work, &["add", "README.md"]);
        git(&work, &["commit", "-m", "initial commit"]);
        Self {
            _temp: temp,
            work,
            remote,
        }
    }

    fn install_policy(&self, policy: &str) {
        let binary = shell_quote(Path::new(env!("CARGO_BIN_EXE_gitguard")));
        self.install_hook(&format!(
            "#!/bin/sh\nexec {binary} hook pre-push --policy {policy} \"$@\"\n"
        ));
    }

    fn install_hook(&self, script: &str) {
        let hook = self.work.join(".git/hooks/pre-push");
        fs::write(&hook, script).expect("install hook in temporary repository");
        let mut permissions = fs::metadata(&hook)
            .expect("read hook metadata")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&hook, permissions).expect("make temporary hook executable");
    }

    fn commit_change(&self, contents: &str) {
        fs::write(self.work.join("README.md"), contents).expect("change test file");
        git(&self.work, &["add", "README.md"]);
        git(&self.work, &["commit", "-m", "test change"]);
    }

    fn push(&self, args: &[&str]) -> Output {
        try_git(&self.work, args)
    }

    fn remote_oid(&self, reference: &str) -> Vec<u8> {
        git(&self.remote, &["rev-parse", reference]).stdout
    }

    fn assert_push_blocked_without_ref_change(&self, policy: &str) {
        self.install_policy("allow");
        assert!(self.push(&["push", "origin", "main"]).status.success());
        let before = self.remote_oid("refs/heads/main");
        self.commit_change(&format!("blocked by {policy}\n"));
        self.install_policy(policy);
        let output = self.push(&["push", "origin", "main"]);
        assert!(
            !output.status.success(),
            "{policy} push unexpectedly succeeded"
        );
        assert_eq!(self.remote_oid("refs/heads/main"), before);
    }
}

fn shell_quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"))
}

fn git(cwd: &Path, args: &[&str]) -> Output {
    let output = try_git(cwd, args);
    assert!(
        output.status.success(),
        "git {args:?} failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn try_git(cwd: &Path, args: &[&str]) -> Output {
    Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .expect("execute git")
}

#[test]
fn allow_policy_completes_a_real_push() {
    let fixture = GitFixture::new();
    fixture.install_policy("allow");
    let output = fixture.push(&["push", "origin", "main"]);
    assert!(
        output.status.success(),
        "allow push failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fixture.remote_oid("refs/heads/main"),
        git(&fixture.work, &["rev-parse", "refs/heads/main"]).stdout
    );
}

#[test]
fn deny_policy_blocks_push_and_preserves_remote_ref() {
    GitFixture::new().assert_push_blocked_without_ref_change("deny");
}

#[test]
fn confirmation_without_a_service_blocks_push_and_preserves_remote_ref() {
    GitFixture::new().assert_push_blocked_without_ref_change("confirm");
}

#[test]
fn unavailable_policy_blocks_push_and_preserves_remote_ref() {
    GitFixture::new().assert_push_blocked_without_ref_change("unavailable");
}

#[test]
fn multiple_refs_are_all_pushed() {
    let fixture = GitFixture::new();
    git(&fixture.work, &["branch", "feature"]);
    fixture.install_policy("allow");
    let output = fixture.push(&["push", "origin", "main", "feature"]);
    assert!(
        output.status.success(),
        "multi-ref push failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    for reference in ["refs/heads/main", "refs/heads/feature"] {
        assert_eq!(
            fixture.remote_oid(reference),
            git(&fixture.work, &["rev-parse", reference]).stdout
        );
    }
}

#[test]
fn branch_creation_and_deletion_work_with_real_git() {
    let fixture = GitFixture::new();
    fixture.install_policy("allow");
    assert!(fixture.push(&["push", "origin", "main"]).status.success());
    git(&fixture.work, &["branch", "topic"]);
    assert!(fixture.push(&["push", "origin", "topic"]).status.success());
    fixture.remote_oid("refs/heads/topic");
    assert!(fixture.push(&["push", "origin", ":topic"]).status.success());
    assert!(!try_git(
        &fixture.remote,
        &["show-ref", "--verify", "refs/heads/topic"]
    )
    .status
    .success());
}

#[test]
fn missing_cli_blocks_push_and_preserves_remote_ref() {
    let fixture = GitFixture::new();
    fixture.install_policy("allow");
    assert!(fixture.push(&["push", "origin", "main"]).status.success());
    let before = fixture.remote_oid("refs/heads/main");
    fixture.commit_change("missing CLI must fail closed\n");
    let missing = shell_quote(&fixture.work.join("does-not-exist/gitguard"));
    fixture.install_hook(&format!(
        "#!/bin/sh\nexec {missing} hook pre-push --policy allow \"$@\"\n"
    ));
    assert!(!fixture.push(&["push", "origin", "main"]).status.success());
    assert_eq!(fixture.remote_oid("refs/heads/main"), before);
}

#[test]
fn hook_execution_failure_blocks_push_and_preserves_remote_ref() {
    let fixture = GitFixture::new();
    fixture.install_policy("allow");
    assert!(fixture.push(&["push", "origin", "main"]).status.success());
    let before = fixture.remote_oid("refs/heads/main");
    fixture.commit_change("failed hook must fail closed\n");
    fixture.install_hook("#!/bin/sh\nexit 42\n");
    assert!(!fixture.push(&["push", "origin", "main"]).status.success());
    assert_eq!(fixture.remote_oid("refs/heads/main"), before);
}
