mod common;

use common::run_code_with_vm;
use php_rs::core::value::{ArrayKey, Val};

#[test]
fn test_error_get_last_includes_location() {
    let code = r#"<?php
$items = null;
foreach ($items as $item) {}
return error_get_last();
"#;

    let (val, vm) = run_code_with_vm(code).expect("code execution failed");
    let Val::Array(arr) = val else {
        panic!("Expected error_get_last() array");
    };

    let line_key = ArrayKey::Str(b"line".to_vec().into());
    let file_key = ArrayKey::Str(b"file".to_vec().into());

    let line_handle = *arr.map.get(&line_key).expect("missing line");
    let file_handle = *arr.map.get(&file_key).expect("missing file");

    let line_val = &vm.arena.get(line_handle).value;
    let file_val = &vm.arena.get(file_handle).value;

    match line_val {
        Val::Int(line) => assert_eq!(*line, 3),
        _ => panic!("Expected int line number"),
    }

    match file_val {
        Val::String(name) => assert_eq!(name.as_ref(), b"Unknown"),
        _ => panic!("Expected file name string"),
    }
}
