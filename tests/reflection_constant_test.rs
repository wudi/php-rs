mod common;
use common::run_php;
use php_rs::core::value::Val;
use std::rc::Rc;

#[test]
fn test_reflection_constant_basic() {
    let result = run_php(r#"<?php
        define('TEST_CONSTANT', 'test value');
        $r = new ReflectionConstant('TEST_CONSTANT');
        return $r->getValue();
    "#);
    assert_eq!(result, Val::String(Rc::new(b"test value".to_vec())));
}

#[test]
fn test_reflection_constant_get_name() {
    let result = run_php(r#"<?php
        define('MY_CONSTANT', 42);
        $r = new ReflectionConstant('MY_CONSTANT');
        return $r->getName();
    "#);
    assert_eq!(result, Val::String(Rc::new(b"MY_CONSTANT".to_vec())));
}

#[test]
fn test_reflection_constant_get_value_int() {
    let result = run_php(r#"<?php
        define('INT_CONST', 123);
        $r = new ReflectionConstant('INT_CONST');
        return $r->getValue();
    "#);
    assert_eq!(result, Val::Int(123));
}

#[test]
fn test_reflection_constant_get_value_float() {
    let result = run_php(r#"<?php
        define('FLOAT_CONST', 3.14);
        $r = new ReflectionConstant('FLOAT_CONST');
        return $r->getValue();
    "#);
    assert_eq!(result, Val::Float(3.14));
}

#[test]
fn test_reflection_constant_get_value_bool() {
    let result = run_php(r#"<?php
        define('BOOL_CONST', true);
        $r = new ReflectionConstant('BOOL_CONST');
        return $r->getValue();
    "#);
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_constant_get_namespace_name() {
    let result = run_php(r#"<?php
        define('SIMPLE', 1);
        $r = new ReflectionConstant('SIMPLE');
        return $r->getNamespaceName();
    "#);
    assert_eq!(result, Val::String(Rc::new(Vec::new())));
}

#[test]
fn test_reflection_constant_get_short_name() {
    let result = run_php(r#"<?php
        define('MY_CONST', 1);
        $r = new ReflectionConstant('MY_CONST');
        return $r->getShortName();
    "#);
    assert_eq!(result, Val::String(Rc::new(b"MY_CONST".to_vec())));
}

#[test]
fn test_reflection_constant_is_deprecated() {
    let result = run_php(r#"<?php
        define('SOME_CONST', 1);
        $r = new ReflectionConstant('SOME_CONST');
        return $r->isDeprecated();
    "#);
    assert_eq!(result, Val::Bool(false));
}

#[test]
fn test_reflection_constant_to_string() {
    let result = run_php(r#"<?php
        define('DEBUG', true);
        $r = new ReflectionConstant('DEBUG');
        return $r->__toString();
    "#);
    if let Val::String(s) = result {
        let output = String::from_utf8_lossy(s.as_ref());
        assert!(output.contains("Constant"));
        assert!(output.contains("DEBUG"));
    } else {
        panic!("Expected string output");
    }
}

#[test]
fn test_reflection_constant_to_string_int() {
    let result = run_php(r#"<?php
        define('MAX_SIZE', 100);
        $r = new ReflectionConstant('MAX_SIZE');
        return $r->__toString();
    "#);
    if let Val::String(s) = result {
        let output = String::from_utf8_lossy(s.as_ref());
        assert!(output.contains("Constant"));
        assert!(output.contains("MAX_SIZE"));
        assert!(output.contains("100"));
    } else {
        panic!("Expected string output");
    }
}

#[test]
fn test_reflection_constant_to_string_string() {
    let result = run_php(r#"<?php
        define('APP_NAME', 'MyApp');
        $r = new ReflectionConstant('APP_NAME');
        return $r->__toString();
    "#);
    if let Val::String(s) = result {
        let output = String::from_utf8_lossy(s.as_ref());
        assert!(output.contains("Constant"));
        assert!(output.contains("APP_NAME"));
        assert!(output.contains("MyApp") || output.contains("'MyApp'"));
    } else {
        panic!("Expected string output");
    }
}

#[test]
fn test_reflection_constant_multiple() {
    let result = run_php(r#"<?php
        define('CONST1', 'value1');
        define('CONST2', 'value2');
        $r1 = new ReflectionConstant('CONST1');
        $r2 = new ReflectionConstant('CONST2');
        return $r1->getValue() . ',' . $r2->getValue();
    "#);
    assert_eq!(result, Val::String(Rc::new(b"value1,value2".to_vec())));
}

#[test]
fn test_reflection_constant_php_built_ins() {
    let result = run_php(r#"<?php
        $r = new ReflectionConstant('PHP_VERSION');
        $val = $r->getValue();
        return is_string($val) && strlen($val) > 0;
    "#);
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_constant_null_value() {
    let result = run_php(r#"<?php
        define('NULL_CONST', null);
        $r = new ReflectionConstant('NULL_CONST');
        return $r->getValue();
    "#);
    assert_eq!(result, Val::Null);
}

#[test]
fn test_reflection_constant_case_sensitive() {
    let result = run_php(r#"<?php
        define('CaseSensitive', 'lower');
        $r = new ReflectionConstant('CaseSensitive');
        return $r->getName();
    "#);
    assert_eq!(result, Val::String(Rc::new(b"CaseSensitive".to_vec())));
}

#[test]
fn test_reflection_constant_get_extension() {
    let result = run_php(r#"<?php
        define('USER_CONST', 'value');
        $r = new ReflectionConstant('USER_CONST');
        return $r->getExtension();
    "#);
    // Should return null for user-defined constants
    assert_eq!(result, Val::Null);
}

#[test]
fn test_reflection_constant_get_extension_name() {
    let result = run_php(r#"<?php
        define('USER_CONST', 'value');
        $r = new ReflectionConstant('USER_CONST');
        return $r->getExtensionName();
    "#);
    // Should return null for user-defined constants
    assert_eq!(result, Val::Null);
}

#[test]
fn test_reflection_constant_get_file_name() {
    let result = run_php(r#"<?php
        define('USER_CONST', 'value');
        $r = new ReflectionConstant('USER_CONST');
        return $r->getFileName();
    "#);
    // Should return null (file tracking not yet implemented)
    assert_eq!(result, Val::Null);
}
