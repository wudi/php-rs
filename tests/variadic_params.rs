mod common;
use common::run_code_capture_output;

#[test]
fn test_variadic_params_collect_args() {
    let code = r#"<?php
        function foo(...$args) {
            var_dump(count($args));
        }
        foo(1, 2, 3);
        foo();
    "#;
    let (_val, output) = run_code_capture_output(code).expect("Execution failed");
    assert!(output.contains("int(3)"));
    assert!(output.contains("int(0)"));
}
