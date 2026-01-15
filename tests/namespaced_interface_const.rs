mod common;
use common::run_code_capture_output;

#[test]
fn test_namespaced_interface_const_access() {
    let code = r#"<?php
        namespace Foo;
        interface Capability { const SSL = 'ssl'; }
        echo Capability::SSL;
    "#;

    let (_val, output) = run_code_capture_output(code).expect("Execution failed");
    assert!(output.contains("ssl"));
}
