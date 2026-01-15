mod common;
use common::run_code_capture_output;

#[test]
fn test_uniqid_lengths() {
    let code = r#"<?php
        var_dump(strlen(uniqid()));
        var_dump(strlen(uniqid('wp_', true)));
    "#;
    let (_val, output) = run_code_capture_output(code).expect("Execution failed");
    assert!(output.contains("int(13)"));
    assert!(output.contains("int(26)"));
}
