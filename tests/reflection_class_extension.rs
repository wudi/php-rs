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

#[test]
fn reflection_class_extension_object() {
    let script = r#"<?php
        $rc = new ReflectionClass('ReflectionClass');
        $ext = $rc->getExtension();
        var_dump($ext instanceof ReflectionExtension);
        var_dump($ext->getName());
    "#;
    let (_val, output) = run_code_capture_output(script).expect("Execution failed");
    assert!(output.contains("bool(true)"));
    assert!(output.contains("string(10) \"Reflection\""));
}
