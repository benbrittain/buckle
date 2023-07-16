#[cfg(test)]
use assert_cmd::Command;

/// Integration test that buckle can download buck2 and run it with same arguments.
#[test]
fn test_buck2_latest() {
    let mut cmd = Command::cargo_bin("buckle").unwrap();
    cmd.arg("--version");
    let assert = cmd.assert();
    let stdout = String::from_utf8(assert.get_output().stdout.to_vec()).unwrap();
    assert!(stdout.starts_with("buck2 "), "found {}", stdout);
    assert.success();
}

/// Integration test that buckle can download buck2 and run it with same arguments with a specified
/// version
#[test]
fn test_buck2_specific_version() {
    let mut cmd = Command::cargo_bin("buckle").unwrap();
    cmd.env("USE_BUCK2_VERSION", "2023-07-15");
    cmd.arg("--version");
    // TODO verify the right version is download after buck2 properly states it's version
    let assert = cmd.assert();
    let stdout = String::from_utf8(assert.get_output().stdout.to_vec()).unwrap();
    assert!(stdout.starts_with("buck2 "), "found {}", stdout);
    assert.success();
}
