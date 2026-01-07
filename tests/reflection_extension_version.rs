mod common;
use common::{run_code_capture_output, run_code_with_vm};
use php_rs::vm::engine::VmError;

#[test]
fn reflection_extension_get_version_returns_string() {
    let script = r#"<?php
        $ext = new ReflectionExtension('reflection');
        var_dump($ext->getVersion());
    "#;
    let (_val, output) = run_code_capture_output(script).expect("Execution failed");
    assert!(output.contains("string("));
}

#[test]
fn reflection_extension_construct_is_case_insensitive() {
    let script = r#"<?php
        $ext = new ReflectionExtension('Reflection');
        var_dump($ext->getName());
    "#;
    let (_val, output) = run_code_capture_output(script).expect("Execution failed");
    assert!(output.contains("string(10) \"Reflection\""));
}

#[test]
fn reflection_extension_construct_unknown_throws() {
    let result = run_code_with_vm(
        r#"<?php
        new ReflectionExtension('no_such_extension');
    "#,
    );
    match result {
        Err(VmError::RuntimeError(msg)) => {
            assert_eq!(msg, "Extension \"no_such_extension\" does not exist");
        }
        Err(other) => panic!("Expected RuntimeError, got {:?}", other),
        Ok(_) => panic!("Expected error, got Ok"),
    }
}
