mod common;

use common::run_code_with_vm;
use php_rs::core::value::{ArrayKey, Val};

#[test]
fn test_static_property_dim_assignment_updates_property() {
    let src = r#"<?php
        class T {
            public static $data = [];
            public static function set() {
                self::$data['key'] = 'value';
                return self::$data;
            }
        }

        return T::set();
    "#;

    let (result, vm) = run_code_with_vm(src).unwrap();
    match result {
        Val::Array(map) => {
            let handle = *map.map.get(&ArrayKey::Str(b"key".to_vec().into())).unwrap();
            let value = vm.arena.get(handle).value.clone();
            assert_eq!(value, Val::String(b"value".to_vec().into()));
        }
        _ => panic!("Expected array"),
    }
}
