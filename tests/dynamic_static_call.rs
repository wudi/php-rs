mod common;
use common::run_code_capture_output;

#[test]
fn test_dynamic_static_call_with_string_class() {
    let code = r#"<?php
        class Foo { public static function test($arg) { echo 'ok'; } }
        $class = Foo::class;
        $class::test([]);
    "#;

    let (_val, output) = run_code_capture_output(code).expect("Execution failed");
    assert!(output.contains("ok"));
}
