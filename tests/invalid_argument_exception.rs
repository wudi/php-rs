mod common;
use common::run_code_capture_output;

#[test]
fn test_invalid_argument_exception_exists() {
    let code = r#"<?php
        var_dump(class_exists('InvalidArgumentException'));
        $e = new InvalidArgumentException('bad');
        var_dump($e instanceof LogicException);
    "#;

    let (_val, output) = run_code_capture_output(code).expect("Execution failed");
    assert!(output.contains("bool(true)"));
}
