mod common;
use common::run_code_capture_output;

#[test]
fn test_is_callable_with_array() {
    let code = r#"<?php
        class Foo { public function bar() {} }
        $obj = new Foo();
        var_dump(is_callable([$obj, 'bar']));
    "#;

    let (_val, output) = run_code_capture_output(code).expect("Execution failed");
    assert!(output.contains("bool(true)"));
}
