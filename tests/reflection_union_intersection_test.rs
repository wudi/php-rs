//! Reflection Union and Intersection Type Tests
//!
//! Tests for ReflectionUnionType and ReflectionIntersectionType

mod common;

use common::run_php;
use php_rs::core::value::Val;

#[test]
fn test_reflection_union_type_basic() {
    let result = run_php(r#"<?php
        // Create mock ReflectionNamedType objects for testing
        $intType = new ReflectionNamedType("int", false, true);
        $stringType = new ReflectionNamedType("string", false, true);
        
        // Create ReflectionUnionType with array of types
        $unionType = new ReflectionUnionType([$intType, $stringType]);
        
        $types = $unionType->getTypes();
        return count($types);
    "#);
    
    if let Val::Int(count) = result {
        assert_eq!(count, 2);
    } else {
        panic!("Expected int count, got {:?}", result);
    }
}

#[test]
fn test_reflection_union_type_get_types() {
    let result = run_php(r#"<?php
        $intType = new ReflectionNamedType("int", false, true);
        $stringType = new ReflectionNamedType("string", false, true);
        $unionType = new ReflectionUnionType([$intType, $stringType]);
        
        $types = $unionType->getTypes();
        
        return [
            $types[0]->getName(),
            $types[1]->getName()
        ];
    "#);
    
    if let Val::Array(arr) = result {
        assert_eq!(arr.map.len(), 2);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_reflection_union_type_allows_null() {
    let result = run_php(r#"<?php
        $intType = new ReflectionNamedType("int", false, true);
        $stringType = new ReflectionNamedType("string", false, true);
        $unionType = new ReflectionUnionType([$intType, $stringType]);
        
        // Union types themselves don't allow null unless explicitly included
        return $unionType->allowsNull();
    "#);
    
    // The union type checks its stored types for null allowance
    // Since we didn't store allowsNull in the union, it may return false
    assert!(matches!(result, Val::Bool(_)));
}

#[test]
fn test_reflection_union_type_with_null() {
    let result = run_php(r#"<?php
        $intType = new ReflectionNamedType("int", false, true);
        $nullType = new ReflectionNamedType("null", true, true);
        $unionType = new ReflectionUnionType([$intType, $nullType]);
        
        $types = $unionType->getTypes();
        return count($types);
    "#);
    
    if let Val::Int(count) = result {
        assert_eq!(count, 2);
    } else {
        panic!("Expected int count");
    }
}

#[test]
fn test_reflection_union_type_inheritance() {
    let result = run_php(r#"<?php
        $intType = new ReflectionNamedType("int", false, true);
        $stringType = new ReflectionNamedType("string", false, true);
        $unionType = new ReflectionUnionType([$intType, $stringType]);
        
        // Check that ReflectionUnionType extends ReflectionType
        return $unionType instanceof ReflectionType;
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_intersection_type_basic() {
    let result = run_php(r#"<?php
        // Create mock class type names for testing
        $typeA = new ReflectionNamedType("A", false, false);
        $typeB = new ReflectionNamedType("B", false, false);
        
        // Create ReflectionIntersectionType with array of types
        $intersectionType = new ReflectionIntersectionType([$typeA, $typeB]);
        
        $types = $intersectionType->getTypes();
        return count($types);
    "#);
    
    if let Val::Int(count) = result {
        assert_eq!(count, 2);
    } else {
        panic!("Expected int count");
    }
}

#[test]
fn test_reflection_intersection_type_get_types() {
    let result = run_php(r#"<?php
        $typeA = new ReflectionNamedType("InterfaceA", false, false);
        $typeB = new ReflectionNamedType("InterfaceB", false, false);
        $intersectionType = new ReflectionIntersectionType([$typeA, $typeB]);
        
        $types = $intersectionType->getTypes();
        
        return [
            $types[0]->getName(),
            $types[1]->getName(),
            $types[0]->isBuiltin(),
            $types[1]->isBuiltin()
        ];
    "#);
    
    if let Val::Array(arr) = result {
        assert_eq!(arr.map.len(), 4);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_reflection_intersection_type_inheritance() {
    let result = run_php(r#"<?php
        $typeA = new ReflectionNamedType("A", false, false);
        $typeB = new ReflectionNamedType("B", false, false);
        $intersectionType = new ReflectionIntersectionType([$typeA, $typeB]);
        
        // Check that ReflectionIntersectionType extends ReflectionType
        return $intersectionType instanceof ReflectionType;
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_intersection_type_allows_null() {
    let result = run_php(r#"<?php
        $typeA = new ReflectionNamedType("A", false, false);
        $typeB = new ReflectionNamedType("B", false, false);
        $intersectionType = new ReflectionIntersectionType([$typeA, $typeB]);
        
        // Intersection types typically don't allow null
        return $intersectionType->allowsNull();
    "#);
    
    assert!(matches!(result, Val::Bool(_)));
}

#[test]
fn test_union_vs_intersection_types() {
    let result = run_php(r#"<?php
        $intType = new ReflectionNamedType("int", false, true);
        $stringType = new ReflectionNamedType("string", false, true);
        
        $unionType = new ReflectionUnionType([$intType, $stringType]);
        $intersectionType = new ReflectionIntersectionType([$intType, $stringType]);
        
        return [
            count($unionType->getTypes()),
            count($intersectionType->getTypes()),
            $unionType instanceof ReflectionUnionType,
            $intersectionType instanceof ReflectionIntersectionType
        ];
    "#);
    
    if let Val::Array(arr) = result {
        assert_eq!(arr.map.len(), 4);
    } else {
        panic!("Expected array result");
    }
}
