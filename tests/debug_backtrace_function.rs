mod common;
use common::run_code_capture_output;

#[test]
fn test_debug_backtrace_basic() {
    let code = r#"<?php
        class Foo {
            public static function bar() {
                $bt = debug_backtrace(DEBUG_BACKTRACE_IGNORE_ARGS, 2);
                var_dump($bt[0]['class']);
                var_dump($bt[0]['function']);
            }
        }
        Foo::bar();
    "#;

    let (_val, output) = run_code_capture_output(code).expect("Execution failed");
    assert!(output.contains("string(3) \"Foo\""));
    assert!(output.contains("string(3) \"bar\""));
}
