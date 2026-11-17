//! Directory Mappings: a nested repository switched to `<none>` is no VCS root of the
//! project, and the enclosing repository's entry for the folder is not reported either.

use ide_core::NullSink;
use ide_host::{Host, HostConfig, ops};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

fn git_init(dir: &Path) {
    std::fs::create_dir_all(dir).unwrap();
    let status = std::process::Command::new("git").args(["init", "-q"]).current_dir(dir).status();
    assert!(status.is_ok_and(|s| s.success()), "git init in {}", dir.display());
}

fn git(dir: &Path, args: &[&str]) {
    let out = std::process::Command::new("git").current_dir(dir).args(args).output().unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
}

async fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    while !done() {
        assert!(tokio::time::Instant::now() < deadline, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_nested_repository_switched_to_none_leaves_the_project() {
    let temp = tempfile::tempdir().unwrap();
    let base = dunce::canonicalize(temp.path()).unwrap();
    let root = base.join("project");
    git_init(&root);
    for (key, value) in [("user.name", "Test"), ("user.email", "test@example.com"), ("commit.gpgsign", "false")] {
        git(&root, &["config", key, value]);
    }
    git_init(&root.join("services/legacy"));
    std::fs::write(root.join("services/legacy/main.rs"), "fn main() {}\n").unwrap();
    std::fs::write(root.join("README.md"), "x\n").unwrap();

    let host = Arc::new(Host::new(HostConfig {
        config_dir: base.join("config"),
        data_dir: base.join("data"),
        local_data_dir: base.join("data"),
        languages: Arc::new(ide_language::LanguageRegistry::load(None).unwrap()),
        syntax: Arc::default(),
        sink_for: Box::new(|_| Arc::new(NullSink)),
        owner: |w| w,
        runtime: tokio::runtime::Handle::current(),
        project_environment: false,
    }));
    host.trust.decide(&root, true, false).unwrap();
    let project = host.open_project("main", &root).unwrap();
    wait_until("both repositories", || project.repos().roots().len() == 2).await;

    let paths = |status: Vec<ide_vcs::FileStatus>| -> Vec<String> { status.into_iter().map(|s| s.rel_path).collect() };
    // `rel_path` is relative to the repository: the nested repository reports its own file.
    let before = paths(ops::vcs::status(&host, "main").await.unwrap());
    assert!(before.iter().any(|p| p == "main.rs"), "{before:?}");
    assert!(before.iter().any(|p| p == "README.md"), "{before:?}");

    ops::project::set_settings(
        &host,
        "main",
        ide_workspace::SettingsScope::Project,
        "vcs.unregisteredRoots",
        r#"["services/legacy"]"#,
    )
    .unwrap();
    wait_until("the unfiled repository", || project.repos().roots().len() == 1).await;
    // Nothing of the unregistered folder reaches the Changes view — not even the enclosing
    // repository's "untracked folder" entry.
    let after = paths(ops::vcs::status(&host, "main").await.unwrap());
    assert!(!after.iter().any(|p| p == "main.rs" || p.starts_with("services/legacy")), "{after:?}");
    assert!(after.iter().any(|p| p == "README.md"), "{after:?}");

    // Switching it back to Git registers the repository again.
    ops::project::set_settings(&host, "main", ide_workspace::SettingsScope::Project, "vcs.unregisteredRoots", "[]")
        .unwrap();
    wait_until("the repository back", || project.repos().roots().len() == 2).await;
    let again = paths(ops::vcs::status(&host, "main").await.unwrap());
    assert!(again.iter().any(|p| p == "main.rs"), "{again:?}");
    host.close_project("main");
}
