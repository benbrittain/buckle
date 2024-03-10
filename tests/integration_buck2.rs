#[cfg(test)]
use assert_cmd::Command;
use std::fs::File;
use std::io::Read;

/// Integration test that buckle can download buck2 and run it with same arguments.
#[test]
fn test_buck2_latest() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("buckle").unwrap();
    cmd.env("BUCKLE_CACHE", tmpdir.path().as_os_str());
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
    let tmpdir = tempfile::TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("buckle").unwrap();
    cmd.env("BUCKLE_CACHE", tmpdir.path().as_os_str());
    cmd.env("USE_BUCK2_VERSION", "2023-07-15");
    cmd.arg("--version");
    let assert = cmd.assert();
    let stdout = String::from_utf8(assert.get_output().stdout.to_vec()).unwrap();
    assert!(stdout.starts_with("buck2 "), "found {}", stdout);
    assert.success();

    // Check we have the buck2 binary
    let expected_binary_path = tmpdir
        .path()
        .join("buckle")
        .join("6f73c2bc7b5b2024e4ecc451feeaded67714e060")
        .join("buck2");
    let binary_file = File::open(&expected_binary_path);
    assert!(
        binary_file.is_ok(),
        "expected file {:?} to exist",
        expected_binary_path
    );

    // Check we have the prelude hash
    let expected_prelude_path = tmpdir
        .path()
        .join("buckle")
        .join("6f73c2bc7b5b2024e4ecc451feeaded67714e060")
        .join("prelude_hash");
    let prelude_hash_file = File::open(&expected_prelude_path);
    assert!(
        prelude_hash_file.is_ok(),
        "expected file {:?} to exist",
        expected_prelude_path
    );

    // Check the prelude hash is as expected for the specified version
    let mut prelude_hash = String::new();
    prelude_hash_file
        .unwrap()
        .read_to_string(&mut prelude_hash)
        .unwrap();
    assert_eq!(
        prelude_hash.trim_end(),
        "be8b3ede73906d6f00055ac6d1caa77f399dcf8f"
    );
}

#[test]
fn test_buck2_fail() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("buckle").unwrap();
    cmd.env("BUCKLE_CACHE", tmpdir.path().as_os_str());
    cmd.arg("--totally-unknown-argument");
    let assert = cmd.assert();
    let stderr = String::from_utf8(assert.get_output().stderr.to_vec()).unwrap();
    assert!(stderr.contains("error: Found argument"), "found {}", stderr);
    assert.failure();
}
