#[cfg(test)]
use assert_cmd::Command;

/// Integration tests that buckle can download bazel and run it with same arguments.
#[test]
fn test_bazel_fromenv() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("buckle").unwrap();
    cmd.env("BUCKLE_HOME", tmpdir.path().as_os_str());
    cmd.env("BUCKLE_BINARY", "bazel");
    cmd.env(
        "BUCKLE_CONFIG",
        r#"
    [archives.bazel]
    source.github.owner = "bazelbuild"
    source.github.repo = "bazel"
    source.github.version = "^7.0.0-pre\\.[0-9.]+$"
    artifact_pattern = "^bazel-%version%-%os%-%arch%$"
    package_type = "single_file"

    [binaries.bazel]
    provided_by = "bazel"
    "#,
    );
    cmd.arg("--version");
    let assert = cmd.assert();
    let stderr = String::from_utf8(assert.get_output().stderr.to_vec()).unwrap();
    assert!(stderr.contains("/buckle/bazel/"), "found {}", stderr);
    let stdout = String::from_utf8(assert.get_output().stdout.to_vec()).unwrap();
    assert!(stdout.starts_with("bazel "), "found {}", stdout);
    assert.success();
}
