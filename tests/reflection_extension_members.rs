mod common;
use common::run_code_capture_output;

#[test]
fn reflection_extension_get_functions() {
    let script = r#"<?php
        $ext = new ReflectionExtension('Core');
        $funcs = $ext->getFunctions();
        var_dump(isset($funcs['strlen']));
        var_dump($funcs['strlen'] instanceof ReflectionFunction);
    "#;
    let (_val, output) = run_code_capture_output(script).expect("Execution failed");
    assert!(output.contains("bool(true)"));
    assert!(output.contains("bool(true)"));
}

#[test]
fn reflection_extension_get_constants() {
    let script = r#"<?php
        $ext = new ReflectionExtension('Core');
        $consts = $ext->getConstants();
        var_dump(isset($consts['STR_PAD_LEFT']));
        var_dump($consts['STR_PAD_LEFT'] === 0);
    "#;
    let (_val, output) = run_code_capture_output(script).expect("Execution failed");
    assert!(output.contains("bool(true)"));
    assert!(output.contains("bool(true)"));
}

#[test]
fn reflection_extension_get_classes() {
    let script = r#"<?php
        $ext = new ReflectionExtension('Reflection');
        $classes = $ext->getClasses();
        var_dump(isset($classes['ReflectionClass']));
        var_dump($classes['ReflectionClass'] instanceof ReflectionClass);
        
        $names = $ext->getClassNames();
        var_dump(in_array('ReflectionClass', $names));
    "#;
    let (_val, output) = run_code_capture_output(script).expect("Execution failed");
    assert!(output.contains("bool(true)"));
    assert!(output.contains("bool(true)"));
    assert!(output.contains("bool(true)"));
}

#[test]
fn reflection_extension_get_dependencies() {
    let script = r#"<?php
        $ext = new ReflectionExtension('Reflection');
        $deps = $ext->getDependencies();
        var_dump(isset($deps['required']));
        var_dump(is_array($deps['required']));
    "#;
    let (_val, output) = run_code_capture_output(script).expect("Execution failed");
    assert!(output.contains("bool(true)"));
    assert!(output.contains("bool(true)"));
}
