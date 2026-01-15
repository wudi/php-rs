mod common;
use common::run_code_capture_output;

#[test]
fn test_argument_unpack_call() {
    let code = r#"<?php
        function foo($a, $b) {
            var_dump($a);
            var_dump($b);
        }
        $args = [1, 2];
        foo(...$args);
    "#;

    let (_val, output) = run_code_capture_output(code).expect("Execution failed");
    assert!(output.contains("int(1)"));
    assert!(output.contains("int(2)"));
    assert!(!output.contains("array(2)"));
    assert!(!output.contains("NULL"));
}
