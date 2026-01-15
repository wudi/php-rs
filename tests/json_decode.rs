mod common;

use common::run_code_with_vm;
use php_rs::core::value::Val;

#[test]
fn test_json_decode_object_default() {
    let src = r#"<?php
        $decoded = json_decode('{"foo":"bar"}');
        return $decoded->foo;
    "#;

    let (result, _vm) = run_code_with_vm(src).unwrap();
    assert_eq!(result, Val::String(b"bar".to_vec().into()));
}

#[test]
fn test_json_decode_object_assoc() {
    let src = r#"<?php
        $decoded = json_decode('{"foo":"bar"}', true);
        return $decoded['foo'];
    "#;

    let (result, _vm) = run_code_with_vm(src).unwrap();
    assert_eq!(result, Val::String(b"bar".to_vec().into()));
}

#[test]
fn test_json_decode_array() {
    let src = r#"<?php
        $decoded = json_decode('[1,2,3]');
        return $decoded[1];
    "#;

    let (result, _vm) = run_code_with_vm(src).unwrap();
    assert_eq!(result, Val::Int(2));
}
