mod common;
use common::run_code_capture_output;

#[test]
fn test_class_const_array_literal() {
    let code = r#"<?php
        class Foo {
            public const OPTIONS = array('a' => 'b');
        }
        echo Foo::OPTIONS['a'];
    "#;
    let (_val, output) = run_code_capture_output(code).expect("Execution failed");
    assert!(output.contains("b"));
}
