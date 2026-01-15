mod common;
use common::run_code_capture_output;

#[test]
fn test_class_const_array_assignment_is_mutable_copy() {
    let code = r#"<?php
        class Foo {
            const OPTS = ['a' => 1];
        }
        $opts = Foo::OPTS;
        $opts['b'] = 2;
        var_dump($opts['b']);
    "#;

    let (_val, output) = run_code_capture_output(code).expect("Execution failed");
    assert!(output.contains("int(2)"));
}
