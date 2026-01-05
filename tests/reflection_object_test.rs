//! ReflectionObject Tests
//!
//! Tests for ReflectionObject class which extends ReflectionClass

mod common;

use common::run_php;
use php_rs::core::value::Val;

#[test]
fn test_reflection_object_basic() {
    let result = run_php(r#"<?php
        class MyClass {
            public $prop = "value";
        }
        
        $obj = new MyClass();
        $ro = new ReflectionObject($obj);
        
        return $ro->getName();
    "#);
    
    assert_eq!(result, Val::String(std::rc::Rc::new(b"MyClass".to_vec())));
}

#[test]
fn test_reflection_object_inherits_from_reflection_class() {
    let result = run_php(r#"<?php
        class TestClass {}
        
        $obj = new TestClass();
        $ro = new ReflectionObject($obj);
        
        return $ro instanceof ReflectionClass;
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_object_get_name() {
    let result = run_php(r#"<?php
        class SomeClass {
            public $x = 10;
        }
        
        $obj = new SomeClass();
        $ro = new ReflectionObject($obj);
        
        return $ro->getName();
    "#);
    
    assert_eq!(result, Val::String(std::rc::Rc::new(b"SomeClass".to_vec())));
}

#[test]
fn test_reflection_object_has_method() {
    let result = run_php(r#"<?php
        class ClassWithMethods {
            public function foo() {}
            private function bar() {}
        }
        
        $obj = new ClassWithMethods();
        $ro = new ReflectionObject($obj);
        
        return [
            $ro->hasMethod('foo'),
            $ro->hasMethod('bar'),
            $ro->hasMethod('baz')
        ];
    "#);
    
    if let Val::Array(arr) = result {
        assert_eq!(arr.map.len(), 3);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_reflection_object_has_property() {
    let result = run_php(r#"<?php
        class ClassWithProps {
            public $publicProp;
            private $privateProp;
        }
        
        $obj = new ClassWithProps();
        $ro = new ReflectionObject($obj);
        
        return [
            $ro->hasProperty('publicProp'),
            $ro->hasProperty('privateProp'),
            $ro->hasProperty('nonexistent')
        ];
    "#);
    
    if let Val::Array(arr) = result {
        assert_eq!(arr.map.len(), 3);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_reflection_object_get_methods() {
    let result = run_php(r#"<?php
        class MyTestClass {
            public function method1() {}
            public function method2() {}
        }
        
        $obj = new MyTestClass();
        $ro = new ReflectionObject($obj);
        $methods = $ro->getMethods();
        
        return count($methods);
    "#);
    
    if let Val::Int(count) = result {
        assert_eq!(count, 2);
    } else {
        panic!("Expected int count");
    }
}

#[test]
fn test_reflection_object_get_properties() {
    let result = run_php(r#"<?php
        class PropClass {
            public $prop1;
            public $prop2;
            private $prop3;
        }
        
        $obj = new PropClass();
        $ro = new ReflectionObject($obj);
        $props = $ro->getProperties();
        
        return count($props);
    "#);
    
    if let Val::Int(count) = result {
        assert_eq!(count, 3);
    } else {
        panic!("Expected int count");
    }
}

#[test]
fn test_reflection_object_is_instantiable() {
    let result = run_php(r#"<?php
        class ConcreteClass {}
        
        $obj = new ConcreteClass();
        $ro = new ReflectionObject($obj);
        
        return $ro->isInstantiable();
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_object_get_namespace_name() {
    let result = run_php(r#"<?php
        class SimpleClass {}
        
        $obj = new SimpleClass();
        $ro = new ReflectionObject($obj);
        
        // Class without namespace should return empty string
        return $ro->getNamespaceName();
    "#);
    
    assert_eq!(result, Val::String(std::rc::Rc::new(b"".to_vec())));
}

#[test]
fn test_reflection_object_get_short_name() {
    let result = run_php(r#"<?php
        class User {}
        
        $obj = new User();
        $ro = new ReflectionObject($obj);
        
        return $ro->getShortName();
    "#);
    
    assert_eq!(result, Val::String(std::rc::Rc::new(b"User".to_vec())));
}

#[test]
#[should_panic(expected = "ReflectionObject::__construct() expects parameter 1 to be object")]
fn test_reflection_object_requires_object() {
    run_php(r#"<?php
        // This should throw an error - not an object
        $ro = new ReflectionObject("NotAnObject");
    "#);
}

#[test]
fn test_reflection_object_vs_reflection_class() {
    let result = run_php(r#"<?php
        class TestClass {
            public $prop = "value";
        }
        
        $obj = new TestClass();
        
        // ReflectionObject takes only object
        $ro = new ReflectionObject($obj);
        
        // ReflectionClass can take object or class name
        $rc = new ReflectionClass($obj);
        
        // Both should return same class name
        return [
            $ro->getName(),
            $rc->getName(),
            $ro->getName() === $rc->getName()
        ];
    "#);
    
    if let Val::Array(arr) = result {
        assert_eq!(arr.map.len(), 3);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_reflection_object_to_string() {
    let result = run_php(r#"<?php
        class StringTestClass {}
        
        $obj = new StringTestClass();
        $ro = new ReflectionObject($obj);
        
        // Should inherit __toString from ReflectionClass
        $str = $ro->__toString();
        return strlen($str) > 0;
    "#);
    
    assert_eq!(result, Val::Bool(true));
}
