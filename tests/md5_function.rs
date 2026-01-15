mod common;
use common::run_code_capture_output;

#[test]
fn test_md5_basic() {
    let code = r#"<?php
        $hash = md5("hello");
        var_dump($hash);
        var_dump(strlen(md5("hello", true)));
    "#;
    let (_val, output) = run_code_capture_output(code).expect("Execution failed");
    assert!(output.contains(r#"string(32) "5d41402abc4b2a76b9719d911017c592""#));
    assert!(output.contains("int(16)"));
}
