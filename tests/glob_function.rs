#[cfg(any(target_os = "linux"))]
mod common;
#[cfg(any(target_os = "linux"))]
use common::run_code_capture_output;
#[cfg(any(target_os = "linux"))]
use tempfile::tempdir;

#[test]
#[cfg(any(target_os = "linux"))]
fn test_glob_onlydir() {
    let temp_dir = tempdir().expect("temp dir");
    std::fs::write(temp_dir.path().join("file.txt"), "x").expect("write");
    std::fs::create_dir(temp_dir.path().join("subdir")).expect("dir");

    let pattern = format!("{}/{}", temp_dir.path().display(), "*");
    let code = format!(
        r#"<?php
        $pattern = "{}";
        $files = glob($pattern);
        $dirs = glob($pattern, GLOB_ONLYDIR);
        var_dump(count($files));
        var_dump(count($dirs));
        var_dump($dirs);
    "#,
        pattern
    );

    let (_val, output) = run_code_capture_output(&code).expect("Execution failed");
    assert!(output.contains("int(2)"));
    assert!(output.contains("int(1)"));
    assert!(output.contains("subdir"));
}
