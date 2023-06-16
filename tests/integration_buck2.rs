#[cfg(test)]
use assert_cmd::Command;

/// Integration tests that buckle can download buck2 and run it with same arguments.
#[test]
fn test_buck2_latest() {
    let mut cmd = Command::cargo_bin("buckle").unwrap();
    cmd.arg("--version");
    let assert = cmd.assert();
    let stderr = String::from_utf8(assert.get_output().stderr.to_vec()).unwrap();
    assert!(stderr.contains("/buckle"), "found {}", stderr);
    let stdout = String::from_utf8(assert.get_output().stdout.to_vec()).unwrap();
    assert!(stdout.starts_with("buck2 "), "found {}", stdout);
    assert.success();
}
