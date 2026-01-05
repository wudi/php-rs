mod common;

use common::run_php;
use php_rs::core::value::Val;

#[test]
fn test_reflection_named_type_basic() {
    let result = run_php(
        r#"<?php
        $type = new ReflectionNamedType('string', false, true);
        return $type->getName();
        "#,
    );
    assert_eq!(result, Val::String(std::rc::Rc::new(b"string".to_vec())));
}

#[test]
fn test_reflection_named_type_is_builtin() {
    let result = run_php(
        r#"<?php
        $type1 = new ReflectionNamedType('string', false, true);
        $type2 = new ReflectionNamedType('MyClass', false, false);
        
        return [
            $type1->isBuiltin(),
            $type2->isBuiltin()
        ];
        "#,
    );
    if let Val::Array(arr) = result {
        let map = &arr.map;
        assert_eq!(map.len(), 2);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_reflection_named_type_allows_null() {
    let result = run_php(
        r#"<?php
        $type1 = new ReflectionNamedType('string', false, true);
        $type2 = new ReflectionNamedType('int', true, true);
        
        return [
            $type1->allowsNull(),
            $type2->allowsNull()
        ];
        "#,
    );
    if let Val::Array(arr) = result {
        let map = &arr.map;
        assert_eq!(map.len(), 2);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_reflection_named_type_to_string() {
    let result = run_php(
        r#"<?php
        $type = new ReflectionNamedType('string', false, true);
        return $type->__toString();
        "#,
    );
    assert_eq!(result, Val::String(std::rc::Rc::new(b"string".to_vec())));
}

#[test]
fn test_reflection_named_type_to_string_nullable() {
    let result = run_php(
        r#"<?php
        $type = new ReflectionNamedType('int', true, true);
        return $type->__toString();
        "#,
    );
    assert_eq!(result, Val::String(std::rc::Rc::new(b"?int".to_vec())));
}

#[test]
fn test_reflection_named_type_class_type() {
    let result = run_php(
        r#"<?php
        class MyClass {}
        
        $type = new ReflectionNamedType('MyClass', false, false);
        
        return [
            $type->getName(),
            $type->isBuiltin()
        ];
        "#,
    );
    if let Val::Array(arr) = result {
        let map = &arr.map;
        assert_eq!(map.len(), 2);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_reflection_type_allows_null_method() {
    let result = run_php(
        r#"<?php
        $type = new ReflectionNamedType('string', true, true);
        return $type->allowsNull();
        "#,
    );
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_named_type_builtin_types() {
    let result = run_php(
        r#"<?php
        $int = new ReflectionNamedType('int', false, true);
        $float = new ReflectionNamedType('float', false, true);
        $bool = new ReflectionNamedType('bool', false, true);
        $array = new ReflectionNamedType('array', false, true);
        
        return [
            $int->getName(),
            $float->getName(),
            $bool->getName(),
            $array->getName()
        ];
        "#,
    );
    if let Val::Array(arr) = result {
        let map = &arr.map;
        assert_eq!(map.len(), 4);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_reflection_named_type_nullable_class() {
    let result = run_php(
        r#"<?php
        class TestClass {}
        
        $type = new ReflectionNamedType('TestClass', true, false);
        
        return [
            $type->getName(),
            $type->allowsNull(),
            $type->isBuiltin(),
            $type->__toString()
        ];
        "#,
    );
    if let Val::Array(arr) = result {
        let map = &arr.map;
        assert_eq!(map.len(), 4);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_reflection_type_inheritance() {
    let result = run_php(
        r#"<?php
        // ReflectionNamedType extends ReflectionType
        // Both allowsNull and __toString should work
        $type = new ReflectionNamedType('string', false, true);
        
        return [
            $type->allowsNull(),  // From ReflectionType
            $type->getName(),      // From ReflectionNamedType
            $type->isBuiltin()     // From ReflectionNamedType
        ];
        "#,
    );
    if let Val::Array(arr) = result {
        let map = &arr.map;
        assert_eq!(map.len(), 3);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_reflection_type_get_name() {
    // Test that ReflectionType::getName() works on base type instances
    let result = run_php(
        r#"<?php
        $type = new ReflectionNamedType('int', false, true);
        return $type->getName();  // Should work through ReflectionType
        "#,
    );
    assert_eq!(result, Val::String(std::rc::Rc::new(b"int".to_vec())));
}
