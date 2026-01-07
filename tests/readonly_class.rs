mod common;

use common::{run_code_capture_output, run_code_with_vm};
use php_rs::core::value::Val;
use php_rs::vm::engine::VmError;

#[test]
fn test_reflection_class_is_readonly() {
    let (result, vm) = run_code_with_vm(r#"<?php
        readonly class Foo {}
        class Bar {}
        return [
            (new ReflectionClass('Foo'))->isReadOnly(),
            (new ReflectionClass('Bar'))->isReadOnly(),
        ];
    "#)
    .expect("execution failed");

    let Val::Array(arr) = result else { panic!("expected array"); };
    let values: Vec<Val> = arr
        .map
        .values()
        .map(|handle| vm.arena.get(*handle).value.clone())
        .collect();

    assert_eq!(values[0], Val::Bool(true));
    assert_eq!(values[1], Val::Bool(false));
}

#[test]
fn test_readonly_class_inheritance_mismatch() {
    let err = run_code_capture_output(r#"<?php
        readonly class Foo {}
        class Bar extends Foo {}
    "#)
    .unwrap_err();

    match err {
        VmError::RuntimeError(message) => {
            assert!(message.contains("Non-readonly class Bar cannot extend readonly class Foo"));
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn test_readonly_class_extends_non_readonly() {
    let err = run_code_capture_output(r#"<?php
        class Foo {}
        readonly class Bar extends Foo {}
    "#)
    .unwrap_err();

    match err {
        VmError::RuntimeError(message) => {
            assert!(message.contains("Readonly class Bar cannot extend non-readonly class Foo"));
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn test_readonly_class_static_property_error() {
    let err = run_code_capture_output(r#"<?php
        readonly class Foo {
            public static int $bar;
        }
    "#)
    .unwrap_err();

    match err {
        VmError::RuntimeError(message) => {
            assert!(message.contains("Static property Foo::$bar cannot be readonly"));
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn test_readonly_class_properties_are_readonly() {
    let err = run_code_capture_output(r#"<?php
        readonly class Foo {
            public int $x;
        }

        $foo = new Foo();
        $foo->x = 1;
        $foo->x = 2;
    "#)
    .unwrap_err();

    match err {
        VmError::RuntimeError(message) => {
            assert!(message.contains("Cannot modify readonly property Foo::$x"));
        }
        other => panic!("unexpected error: {other:?}"),
    }
}
