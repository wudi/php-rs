mod common;
use common::run_code_capture_output;

#[test]
fn reflection_class_extension_for_internal_class() {
    let script = r#"<?php
        $rc = new ReflectionClass('ReflectionClass');
        var_dump($rc->getExtensionName());
    "#;
    let (_val, output) = run_code_capture_output(script).expect("Execution failed");
    assert!(output.contains("string(10) \"Reflection\""));
}

#[test]
fn reflection_class_extension_for_user_class() {
    let script = r#"<?php
        class Foo {}
        $rc = new ReflectionClass('Foo');
        var_dump($rc->getExtensionName());
    "#;
    let (_val, output) = run_code_capture_output(script).expect("Execution failed");
    assert!(output.contains("bool(false)"));
}
