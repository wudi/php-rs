mod common;
use common::run_code_capture_output;

#[test]
fn test_debug_backtrace_constants() {
    let code = r#"<?php
        var_dump(DEBUG_BACKTRACE_PROVIDE_OBJECT);
        var_dump(DEBUG_BACKTRACE_IGNORE_ARGS);
    "#;

    let (_val, output) = run_code_capture_output(code).expect("Execution failed");
    assert!(output.contains("int(1)"));
    assert!(output.contains("int(2)"));
}
