mod common;

use common::run_code_with_vm;
use php_rs::core::value::Val;

#[test]
fn test_urlencode_null_coerces_to_empty_string() {
    let src = r#"<?php
        return urlencode(null);
    "#;

    let (result, _vm) = run_code_with_vm(src).unwrap();
    assert_eq!(result, Val::String(Vec::new().into()));
}
