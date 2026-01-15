mod common;
use common::run_code_capture_output;

#[test]
fn test_autoload_on_class_const_fetch() {
    let code = r#"<?php
        spl_autoload_register(function($name) {
            if ($name === 'Foo') {
                eval('class Foo { const BAR = 123; }');
            }
        });
        echo Foo::BAR;
    "#;

    let (_val, output) = run_code_capture_output(code).expect("Execution failed");
    assert!(output.contains("123"));
}
