mod common;

use common::run_code_capture_output;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

fn temp_dir_path(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("php_vm_test_{}", name));
    path
}

#[test]
fn test_is_writable_nonexistent_child_in_readonly_dir() {
    let dir = temp_dir_path("is_writable_ro_dir");
    let child = dir.join("child.txt");

    fs::create_dir_all(&dir).expect("create dir");
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o555)).expect("chmod dir");

    let code = format!(
        r#"<?php echo is_writable("{}") ? 'yes' : 'no';"#,
        child.display()
    );

    let (_, output) = run_code_capture_output(&code).expect("execution failed");
    assert_eq!(output, "no");

    fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).ok();
    fs::remove_dir_all(&dir).ok();
}
