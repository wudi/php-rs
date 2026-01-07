mod common;

use common::run_code_with_vm;
use php_rs::core::value::Val;
use std::rc::Rc;

#[test]
fn test_reflection_class_get_doc_comment() {
    let (result, vm) = run_code_with_vm(r#"<?php
        /** Class doc */
        class DocClass {}

        /** Interface doc */
        interface DocInterface {}

        /** Trait doc */
        trait DocTrait {}

        class NoDoc {}

        return [
            (new ReflectionClass('DocClass'))->getDocComment(),
            (new ReflectionClass('DocInterface'))->getDocComment(),
            (new ReflectionClass('DocTrait'))->getDocComment(),
            (new ReflectionClass('NoDoc'))->getDocComment(),
        ];
    "#)
    .expect("execution failed");

    let Val::Array(arr) = result else { panic!("expected array"); };

    let values: Vec<Val> = arr
        .map
        .values()
        .map(|handle| vm.arena.get(*handle).value.clone())
        .collect();

    assert_eq!(values[0], Val::String(Rc::new(b"/** Class doc */".to_vec())));
    assert_eq!(values[1], Val::String(Rc::new(b"/** Interface doc */".to_vec())));
    assert_eq!(values[2], Val::String(Rc::new(b"/** Trait doc */".to_vec())));
    assert_eq!(values[3], Val::Bool(false));
}

#[test]
fn test_reflection_property_and_constant_doc_comment() {
    let (result, vm) = run_code_with_vm(r#"<?php
        class DocHolder {
            /** prop doc */
            public $a;
            public $b;

            /** static doc */
            public static $s;

            /** shared doc */
            public $x, $y;

            /** const doc */
            public const FOO = 1;
            public const BAR = 2;
        }

        return [
            (new ReflectionProperty('DocHolder', 'a'))->getDocComment(),
            (new ReflectionProperty('DocHolder', 'b'))->getDocComment(),
            (new ReflectionProperty('DocHolder', 's'))->getDocComment(),
            (new ReflectionProperty('DocHolder', 'x'))->getDocComment(),
            (new ReflectionProperty('DocHolder', 'y'))->getDocComment(),
            (new ReflectionClassConstant('DocHolder', 'FOO'))->getDocComment(),
            (new ReflectionClassConstant('DocHolder', 'BAR'))->getDocComment(),
        ];
    "#)
    .expect("execution failed");

    let Val::Array(arr) = result else { panic!("expected array"); };

    let values: Vec<Val> = arr
        .map
        .values()
        .map(|handle| vm.arena.get(*handle).value.clone())
        .collect();

    assert_eq!(values[0], Val::String(Rc::new(b"/** prop doc */".to_vec())));
    assert_eq!(values[1], Val::Bool(false));
    assert_eq!(values[2], Val::String(Rc::new(b"/** static doc */".to_vec())));
    assert_eq!(values[3], Val::String(Rc::new(b"/** shared doc */".to_vec())));
    assert_eq!(values[4], Val::String(Rc::new(b"/** shared doc */".to_vec())));
    assert_eq!(values[5], Val::String(Rc::new(b"/** const doc */".to_vec())));
    assert_eq!(values[6], Val::Bool(false));
}
