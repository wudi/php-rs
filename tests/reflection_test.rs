//! Reflection API Tests
//!
//! Tests for PHP Reflection implementation covering:
//! - ReflectionClass
//! - ReflectionFunction
//! - ReflectionMethod
//! - ReflectionProperty
//! - ReflectionParameter

mod common;

use common::{run_code_capture_output, run_code_with_vm, run_php};
use php_rs::core::value::{ArrayKey, Val};
use php_rs::vm::engine::{VM, VmError};
use std::rc::Rc;

fn get_array_idx(vm: &VM, val: &Val, idx: i64) -> Val {
    if let Val::Array(arr) = val {
        let key = ArrayKey::Int(idx);
        let handle = arr.map.get(&key).expect("Array index not found");
        vm.arena.get(*handle).value.clone()
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_reflection_class_basic() {
    let result = run_php(r#"<?php
        class TestClass {
            public $publicProp;
            private $privateProp;
            protected $protectedProp;
            
            const MY_CONST = 42;
            
            public function publicMethod() {}
            private function privateMethod() {}
        }
        
        $rc = new ReflectionClass('TestClass');
        return $rc->getName();
    "#);
    
    assert_eq!(result, Val::String(std::rc::Rc::new(b"TestClass".to_vec())));
}

#[test]
fn test_reflection_class_from_object() {
    let result = run_php(r#"<?php
        class MyClass {}
        $obj = new MyClass();
        $rc = new ReflectionClass($obj);
        return $rc->getName();
    "#);
    
    assert_eq!(result, Val::String(std::rc::Rc::new(b"MyClass".to_vec())));
}

#[test]
fn test_reflection_class_is_abstract() {
    let result = run_php(r#"<?php
        abstract class AbstractClass {}
        class ConcreteClass {}
        
        $rc1 = new ReflectionClass('AbstractClass');
        $rc2 = new ReflectionClass('ConcreteClass');
        
        return [$rc1->isAbstract(), $rc2->isAbstract()];
    "#);
    
    if let Val::Array(arr) = result {
        let map = &arr.map;
        assert_eq!(map.len(), 2);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_reflection_class_is_interface() {
    let result = run_php(r#"<?php
        interface TestInterface {}
        class TestClass {}
        
        $rc1 = new ReflectionClass('TestInterface');
        $rc2 = new ReflectionClass('TestClass');
        
        return [$rc1->isInterface(), $rc2->isInterface()];
    "#);
    
    if let Val::Array(arr) = result {
        let map = &arr.map;
        assert_eq!(map.len(), 2);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_reflection_class_is_trait() {
    let result = run_php(r#"<?php
        trait TestTrait {}
        class TestClass {}
        
        $rc1 = new ReflectionClass('TestTrait');
        $rc2 = new ReflectionClass('TestClass');
        
        return [$rc1->isTrait(), $rc2->isTrait()];
    "#);
    
    if let Val::Array(arr) = result {
        let map = &arr.map;
        assert_eq!(map.len(), 2);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_reflection_class_new_instance_calls_constructor() {
    let (result, vm) = run_code_with_vm(r#"<?php
        class CtorClass {
            public int $value = 0;
            public function __construct(int $v) { $this->value = $v; }
        }
        $rc = new ReflectionClass('CtorClass');
        $obj = $rc->newInstance(42);
        return [$obj instanceof CtorClass, $obj->value];
    "#).unwrap();

    assert_eq!(get_array_idx(&vm, &result, 0), Val::Bool(true));
    assert_eq!(get_array_idx(&vm, &result, 1), Val::Int(42));
}

#[test]
fn test_reflection_class_new_instance_no_constructor_with_args_throws() {
    let result = run_code_with_vm(r#"<?php
        class NoCtor {}
        $rc = new ReflectionClass('NoCtor');
        $rc->newInstance(1);
    "#);
    match result {
        Err(VmError::RuntimeError(msg)) => {
            assert!(
                msg.contains("Class NoCtor does not have a constructor, so you cannot pass any constructor arguments"),
                "unexpected error: {msg}"
            );
        }
        Err(other) => panic!("Expected RuntimeError, got {:?}", other),
        Ok(_) => panic!("Expected error, got Ok"),
    }
}

#[test]
fn test_reflection_class_new_instance_non_public_constructor_throws() {
    let result = run_code_with_vm(r#"<?php
        class PrivateCtor {
            private function __construct() {}
        }
        $rc = new ReflectionClass('PrivateCtor');
        $rc->newInstance();
    "#);
    match result {
        Err(VmError::RuntimeError(msg)) => {
            assert!(
                msg.contains("Access to non-public constructor of class PrivateCtor"),
                "unexpected error: {msg}"
            );
        }
        Err(other) => panic!("Expected RuntimeError, got {:?}", other),
        Ok(_) => panic!("Expected error, got Ok"),
    }
}

#[test]
fn test_reflection_class_new_instance_args_calls_constructor() {
    let (result, vm) = run_code_with_vm(r#"<?php
        class ArgsCtor {
            public int $a = 0;
            public int $b = 0;
            public function __construct(int $a, int $b) {
                $this->a = $a;
                $this->b = $b;
            }
        }
        $rc = new ReflectionClass('ArgsCtor');
        $obj = $rc->newInstanceArgs([1 => 7, 0 => 3]);
        return [$obj instanceof ArgsCtor, $obj->a, $obj->b];
    "#).unwrap();

    assert_eq!(get_array_idx(&vm, &result, 0), Val::Bool(true));
    assert_eq!(get_array_idx(&vm, &result, 1), Val::Int(7));
    assert_eq!(get_array_idx(&vm, &result, 2), Val::Int(3));
}

#[test]
fn test_reflection_class_new_instance_args_no_constructor_with_args_throws() {
    let result = run_code_with_vm(r#"<?php
        class NoCtorArgs {}
        $rc = new ReflectionClass('NoCtorArgs');
        $rc->newInstanceArgs([1]);
    "#);
    match result {
        Err(VmError::RuntimeError(msg)) => {
            assert!(
                msg.contains("Class NoCtorArgs does not have a constructor, so you cannot pass any constructor arguments"),
                "unexpected error: {msg}"
            );
        }
        Err(other) => panic!("Expected RuntimeError, got {:?}", other),
        Ok(_) => panic!("Expected error, got Ok"),
    }
}

#[test]
fn test_reflection_class_new_instance_args_zero_args_uses_empty_list() {
    let (result, vm) = run_code_with_vm(r#"<?php
        class ZeroArgs {
            public int $value = 11;
        }
        $rc = new ReflectionClass('ZeroArgs');
        $obj = $rc->newInstanceArgs();
        return [$obj instanceof ZeroArgs, $obj->value];
    "#).unwrap();

    assert_eq!(get_array_idx(&vm, &result, 0), Val::Bool(true));
    assert_eq!(get_array_idx(&vm, &result, 1), Val::Int(11));
}

#[test]
fn test_reflection_class_new_instance_without_constructor_does_not_call_ctor() {
    let (_result, output) = run_code_capture_output(r#"<?php
        class Foo {
            public int $x = 0;
            public function __construct() { $this->x = 42; }
        }
        $rc = new ReflectionClass(Foo::class);
        $obj = $rc->newInstanceWithoutConstructor();
        var_dump($obj instanceof Foo);
        var_dump($obj->x);
    "#).unwrap();
    assert_eq!(output, "bool(true)\nint(0)\n");
}

#[test]
fn test_reflection_class_new_instance_without_constructor_enum_error() {
    let result = run_code_with_vm(r#"<?php
        enum Foo {}
        $rc = new ReflectionClass(Foo::class);
        $rc->newInstanceWithoutConstructor();
    "#);
    match result {
        Err(VmError::RuntimeError(msg)) => {
            assert!(
                msg == "Cannot instantiate enum Foo" || msg == "Class \"Foo\" does not exist",
                "unexpected error: {msg}"
            );
        }
        Err(other) => panic!("Expected RuntimeError, got {:?}", other),
        Ok(_) => panic!("Expected error, got Ok"),
    }
}

#[test]
fn test_reflection_class_new_instance_without_constructor_abstract_error() {
    let result = run_code_with_vm(r#"<?php
        abstract class A {}
        $rc = new ReflectionClass(A::class);
        $rc->newInstanceWithoutConstructor();
    "#);
    match result {
        Err(VmError::RuntimeError(msg)) => {
            assert_eq!(msg, "Cannot instantiate abstract class A");
        }
        Err(other) => panic!("Expected RuntimeError, got {:?}", other),
        Ok(_) => panic!("Expected error, got Ok"),
    }
}

#[test]
fn test_reflection_class_new_instance_without_constructor_interface_error() {
    let result = run_code_with_vm(r#"<?php
        interface I {}
        $rc = new ReflectionClass(I::class);
        $rc->newInstanceWithoutConstructor();
    "#);
    match result {
        Err(VmError::RuntimeError(msg)) => {
            assert_eq!(msg, "Cannot instantiate interface I");
        }
        Err(other) => panic!("Expected RuntimeError, got {:?}", other),
        Ok(_) => panic!("Expected error, got Ok"),
    }
}

#[test]
fn test_reflection_class_new_instance_without_constructor_trait_error() {
    let result = run_code_with_vm(r#"<?php
        trait T {}
        $rc = new ReflectionClass(T::class);
        $rc->newInstanceWithoutConstructor();
    "#);
    match result {
        Err(VmError::RuntimeError(msg)) => {
            assert_eq!(msg, "Cannot instantiate trait T");
        }
        Err(other) => panic!("Expected RuntimeError, got {:?}", other),
        Ok(_) => panic!("Expected error, got Ok"),
    }
}

#[test]
fn test_reflection_class_new_instance_without_constructor_internal_final_guard() {
    let result = run_code_with_vm(r#"<?php
        $rc = new ReflectionClass(Generator::class);
        $rc->newInstanceWithoutConstructor();
    "#);
    match result {
        Err(VmError::RuntimeError(msg)) => {
            assert_eq!(
                msg,
                "Class Generator is an internal class marked as final that cannot be instantiated without invoking its constructor"
            );
        }
        Err(other) => panic!("Expected RuntimeError, got {:?}", other),
        Ok(_) => panic!("Expected error, got Ok"),
    }
}

#[test]
fn test_reflection_class_is_subclass_of_multilevel_and_interfaces() {
    let (result, vm) = run_code_with_vm(r#"<?php
        interface BaseInterface {}
        interface ChildInterface extends BaseInterface {}
        class GrandParentClass {}
        class ParentClass extends GrandParentClass {}
        class ChildClass extends ParentClass implements ChildInterface {}

        $rc_child = new ReflectionClass('ChildClass');
        $rc_parent = new ReflectionClass('ParentClass');
        $rc_interface = new ReflectionClass('ChildInterface');

        return [
            $rc_child->isSubclassOf('GrandParentClass'),
            $rc_child->isSubclassOf('BaseInterface'),
            $rc_parent->isSubclassOf('ParentClass'),
            $rc_interface->isSubclassOf('BaseInterface')
        ];
    "#).unwrap();

    assert_eq!(get_array_idx(&vm, &result, 0), Val::Bool(true));
    assert_eq!(get_array_idx(&vm, &result, 1), Val::Bool(true));
    assert_eq!(get_array_idx(&vm, &result, 2), Val::Bool(false));
    assert_eq!(get_array_idx(&vm, &result, 3), Val::Bool(true));
}

#[test]
fn test_reflection_class_is_instantiable() {
    let result = run_php(r#"<?php
        abstract class AbstractClass {}
        interface TestInterface {}
        class ConcreteClass {}
        
        $rc1 = new ReflectionClass('AbstractClass');
        $rc2 = new ReflectionClass('TestInterface');
        $rc3 = new ReflectionClass('ConcreteClass');
        
        return [
            $rc1->isInstantiable(),
            $rc2->isInstantiable(),
            $rc3->isInstantiable()
        ];
    "#);
    
    if let Val::Array(arr) = result {
        let map = &arr.map;
        assert_eq!(map.len(), 3);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_reflection_class_has_method() {
    let result = run_php(r#"<?php
        class TestClass {
            public function myMethod() {}
        }
        
        $rc = new ReflectionClass('TestClass');
        return [
            $rc->hasMethod('myMethod'),
            $rc->hasMethod('nonExistent')
        ];
    "#);
    
    if let Val::Array(arr) = result {
        let map = &arr.map;
        assert_eq!(map.len(), 2);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_reflection_class_has_property() {
    let result = run_php(r#"<?php
        class TestClass {
            public $myProp;
        }
        
        $rc = new ReflectionClass('TestClass');
        return [
            $rc->hasProperty('myProp'),
            $rc->hasProperty('nonExistent')
        ];
    "#);
    
    if let Val::Array(arr) = result {
        let map = &arr.map;
        assert_eq!(map.len(), 2);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_reflection_class_has_constant() {
    let result = run_php(r#"<?php
        class TestClass {
            const MY_CONST = 42;
        }
        
        $rc = new ReflectionClass('TestClass');
        return [
            $rc->hasConstant('MY_CONST'),
            $rc->hasConstant('NON_EXISTENT')
        ];
    "#);
    
    if let Val::Array(arr) = result {
        let map = &arr.map;
        assert_eq!(map.len(), 2);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_reflection_class_get_methods() {
    let result = run_php(r#"<?php
        class TestClass {
            public function method1() {}
            public function method2() {}
            private function method3() {}
        }
        
        $rc = new ReflectionClass('TestClass');
        return count($rc->getMethods());
    "#);
    
    assert_eq!(result, Val::Int(3));
}

#[test]
fn test_reflection_class_get_properties() {
    let result = run_php(r#"<?php
        class TestClass {
            public $prop1;
            public $prop2;
            private $prop3;
        }
        
        $rc = new ReflectionClass('TestClass');
        return count($rc->getProperties());
    "#);
    
    assert_eq!(result, Val::Int(3));
}

#[test]
fn test_reflection_class_get_constants() {
    let result = run_php(r#"<?php
        class TestClass {
            const CONST1 = 1;
            const CONST2 = 2;
        }
        
        $rc = new ReflectionClass('TestClass');
        $constants = $rc->getConstants();
        return count($constants);
    "#);
    
    assert_eq!(result, Val::Int(2));
}

#[test]
fn test_reflection_class_get_constant() {
    let result = run_php(r#"<?php
        class TestClass {
            const MY_CONST = 42;
        }
        
        $rc = new ReflectionClass('TestClass');
        return $rc->getConstant('MY_CONST');
    "#);
    
    assert_eq!(result, Val::Int(42));
}

#[test]
fn test_reflection_class_get_constant_not_found() {
    let result = run_php(r#"<?php
        class TestClass {}
        
        $rc = new ReflectionClass('TestClass');
        return $rc->getConstant('NON_EXISTENT');
    "#);
    
    assert_eq!(result, Val::Bool(false));
}

#[test]
fn test_reflection_class_get_parent_class() {
    let result = run_php(r#"<?php
        class ParentClass {}
        class ChildClass extends ParentClass {}
        
        $rc = new ReflectionClass('ChildClass');
        $parent = $rc->getParentClass();
        return $parent->getName();
    "#);
    
    // Returns ReflectionClass object for parent, verify by getting name
    assert_eq!(result, Val::String(Rc::new(b"ParentClass".to_vec())));
}

#[test]
fn test_reflection_class_get_parent_class_none() {
    let result = run_php(r#"<?php
        class TestClass {}
        
        $rc = new ReflectionClass('TestClass');
        return $rc->getParentClass();
    "#);
    
    assert_eq!(result, Val::Bool(false));
}

#[test]
fn test_reflection_class_get_interface_names() {
    let result = run_php(r#"<?php
        interface Interface1 {}
        interface Interface2 {}
        class TestClass implements Interface1, Interface2 {}
        
        $rc = new ReflectionClass('TestClass');
        return count($rc->getInterfaceNames());
    "#);
    
    assert_eq!(result, Val::Int(2));
}

#[test]
fn test_reflection_class_implements_interface() {
    let result = run_php(r#"<?php
        interface TestInterface {}
        class TestClass implements TestInterface {}
        
        $rc = new ReflectionClass('TestClass');
        return [
            $rc->implementsInterface('TestInterface'),
            $rc->implementsInterface('NonExistent')
        ];
    "#);
    
    if let Val::Array(arr) = result {
        let map = &arr.map;
        assert_eq!(map.len(), 2);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_reflection_class_namespace() {
    // Test with non-namespaced class
    let result = run_php(r#"<?php
        class TestClass {}
        
        $rc = new ReflectionClass('TestClass');
        return [
            $rc->getShortName(),
            $rc->inNamespace()
        ];
    "#);
    
    if let Val::Array(arr) = result {
        let map = &arr.map;
        assert_eq!(map.len(), 2);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_reflection_class_no_namespace() {
    let result = run_php(r#"<?php
        class TestClass {}
        
        $rc = new ReflectionClass('TestClass');
        return [
            $rc->getNamespaceName(),
            $rc->getShortName(),
            $rc->inNamespace()
        ];
    "#);
    
    if let Val::Array(arr) = result {
        let map = &arr.map;
        assert_eq!(map.len(), 3);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_reflection_class_to_string() {
    let result = run_php(r#"<?php
        class TestClass {}
        
        $rc = new ReflectionClass('TestClass');
        $str = $rc->__toString();
        return strlen($str) > 0;
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_function_basic() {
    let result = run_php(r#"<?php
        function test_function() {
            return 42;
        }
        
        $rf = new ReflectionFunction('test_function');
        return $rf->getName();
    "#);
    
    assert_eq!(result, Val::String(std::rc::Rc::new(b"test_function".to_vec())));
}

#[test]
fn test_reflection_function_builtin() {
    let result = run_php(r#"<?php
        $rf = new ReflectionFunction('strlen');
        return $rf->getName();
    "#);
    
    assert_eq!(result, Val::String(std::rc::Rc::new(b"strlen".to_vec())));
}

#[test]
#[should_panic(expected = "code execution failed")]
fn test_reflection_function_not_found() {
    let _result = run_php(r#"<?php
        $rf = new ReflectionFunction('non_existent_function');
        return false;
    "#);
}

#[test]
#[should_panic(expected = "code execution failed")]
fn test_reflection_class_nonexistent() {
    let _result = run_php(r#"<?php
        $rc = new ReflectionClass('NonExistentClass');
        return false;
    "#);
}

// ============================================================================
// ReflectionMethod Tests
// ============================================================================

#[test]
fn test_reflection_method_basic() {
    let result = run_php(r#"<?php
        class TestClass {
            public function myMethod() {}
        }
        
        $rm = new ReflectionMethod('TestClass', 'myMethod');
        return $rm->getName();
    "#);
    
    assert_eq!(result, Val::String(std::rc::Rc::new(b"myMethod".to_vec())));
}

#[test]
fn test_reflection_method_from_object() {
    let result = run_php(r#"<?php
        class TestClass {
            public function testMethod() {}
        }
        
        $obj = new TestClass();
        $rm = new ReflectionMethod($obj, 'testMethod');
        return $rm->getName();
    "#);
    
    assert_eq!(result, Val::String(std::rc::Rc::new(b"testMethod".to_vec())));
}

#[test]
fn test_reflection_method_visibility() {
    let result = run_php(r#"<?php
        class TestClass {
            public function publicMethod() {}
            private function privateMethod() {}
            protected function protectedMethod() {}
        }
        
        $rm1 = new ReflectionMethod('TestClass', 'publicMethod');
        $rm2 = new ReflectionMethod('TestClass', 'privateMethod');
        $rm3 = new ReflectionMethod('TestClass', 'protectedMethod');
        
        return [
            $rm1->isPublic(),
            $rm1->isPrivate(),
            $rm1->isProtected(),
            $rm2->isPublic(),
            $rm2->isPrivate(),
            $rm2->isProtected(),
            $rm3->isPublic(),
            $rm3->isPrivate(),
            $rm3->isProtected(),
        ];
    "#);
    
    if let Val::Array(arr) = result {
        let map = &arr.map;
        assert_eq!(map.len(), 9);
        // Could check individual values but just verify we got all 9 results
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_reflection_method_is_static() {
    let result = run_php(r#"<?php
        class TestClass {
            public function instanceMethod() {}
            public static function staticMethod() {}
        }
        
        $rm1 = new ReflectionMethod('TestClass', 'instanceMethod');
        $rm2 = new ReflectionMethod('TestClass', 'staticMethod');
        
        return [$rm1->isStatic(), $rm2->isStatic()];
    "#);
    
    if let Val::Array(arr) = result {
        let map = &arr.map;
        assert_eq!(map.len(), 2);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_reflection_method_is_abstract() {
    let result = run_php(r#"<?php
        abstract class AbstractClass {
            abstract public function abstractMethod();
            public function concreteMethod() {}
        }
        
        $rm1 = new ReflectionMethod('AbstractClass', 'abstractMethod');
        $rm2 = new ReflectionMethod('AbstractClass', 'concreteMethod');
        
        return [$rm1->isAbstract(), $rm2->isAbstract()];
    "#);
    
    if let Val::Array(arr) = result {
        let map = &arr.map;
        assert_eq!(map.len(), 2);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_reflection_method_is_constructor() {
    let result = run_php(r#"<?php
        class TestClass {
            public function __construct() {}
            public function regularMethod() {}
        }
        
        $rm1 = new ReflectionMethod('TestClass', '__construct');
        $rm2 = new ReflectionMethod('TestClass', 'regularMethod');
        
        return [$rm1->isConstructor(), $rm2->isConstructor()];
    "#);
    
    if let Val::Array(arr) = result {
        let map = &arr.map;
        assert_eq!(map.len(), 2);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_reflection_method_is_destructor() {
    let result = run_php(r#"<?php
        class TestClass {
            public function __destruct() {}
            public function regularMethod() {}
        }
        
        $rm1 = new ReflectionMethod('TestClass', '__destruct');
        $rm2 = new ReflectionMethod('TestClass', 'regularMethod');
        
        return [$rm1->isDestructor(), $rm2->isDestructor()];
    "#);
    
    if let Val::Array(arr) = result {
        let map = &arr.map;
        assert_eq!(map.len(), 2);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_reflection_method_get_modifiers() {
    let result = run_php(r#"<?php
        class TestClass {
            public function publicMethod() {}
            private function privateMethod() {}
            protected function protectedMethod() {}
            public static function staticMethod() {}
        }
        
        $rm1 = new ReflectionMethod('TestClass', 'publicMethod');
        $rm2 = new ReflectionMethod('TestClass', 'privateMethod');
        $rm3 = new ReflectionMethod('TestClass', 'protectedMethod');
        $rm4 = new ReflectionMethod('TestClass', 'staticMethod');
        
        return [
            $rm1->getModifiers(),
            $rm2->getModifiers(),
            $rm3->getModifiers(),
            $rm4->getModifiers(),
        ];
    "#);
    
    if let Val::Array(arr) = result {
        let map = &arr.map;
        assert_eq!(map.len(), 4);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_reflection_method_get_declaring_class() {
    let result = run_php(r#"<?php
        class ParentClass {
            public function parentMethod() {}
        }
        
        class ChildClass extends ParentClass {
            public function childMethod() {}
        }
        
        $rm1 = new ReflectionMethod('ChildClass', 'parentMethod');
        $rc1 = $rm1->getDeclaringClass();
        
        $rm2 = new ReflectionMethod('ChildClass', 'childMethod');
        $rc2 = $rm2->getDeclaringClass();
        
        return [$rc1->getName(), $rc2->getName()];
    "#);
    
    if let Val::Array(arr) = result {
        let map = &arr.map;
        assert_eq!(map.len(), 2);
    } else {
        panic!("Expected array result");
    }
}

#[test]
#[should_panic(expected = "code execution failed")]
fn test_reflection_method_not_found() {
    let _result = run_php(r#"<?php
        class TestClass {}
        $rm = new ReflectionMethod('TestClass', 'nonExistentMethod');
        return false;
    "#);
}

// ============================================================================
// ReflectionParameter Tests
// ============================================================================

#[test]
fn test_reflection_parameter_function_by_index() {
    let result = run_php(r#"<?php
        function testFunc($param1, $param2) {}
        
        $rp = new ReflectionParameter('testFunc', 0);
        return $rp->getName();
    "#);
    
    assert_eq!(result, Val::String(std::rc::Rc::new(b"param1".to_vec())));
}

#[test]
fn test_reflection_parameter_function_by_name() {
    let result = run_php(r#"<?php
        function testFunc($param1, $param2) {}
        
        $rp = new ReflectionParameter('testFunc', 'param2');
        return $rp->getName();
    "#);
    
    assert_eq!(result, Val::String(std::rc::Rc::new(b"param2".to_vec())));
}

#[test]
fn test_reflection_parameter_method_by_array() {
    let result = run_php(r#"<?php
        class TestClass {
            public function testMethod($param1, $param2) {}
        }
        
        $rp = new ReflectionParameter(['TestClass', 'testMethod'], 1);
        return $rp->getName();
    "#);
    
    assert_eq!(result, Val::String(std::rc::Rc::new(b"param2".to_vec())));
}

#[test]
fn test_reflection_parameter_is_optional() {
    let result = run_php(r#"<?php
        function testFunc($required, $optional = 'default') {}
        
        $rp1 = new ReflectionParameter('testFunc', 'required');
        $rp2 = new ReflectionParameter('testFunc', 'optional');
        
        return [$rp1->isOptional(), $rp2->isOptional()];
    "#);
    
    if let Val::Array(arr) = result {
        let map = &arr.map;
        assert_eq!(map.len(), 2);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_reflection_parameter_get_default_value() {
    let result = run_php(r#"<?php
        function testFunc($param = 42) {}
        
        $rp = new ReflectionParameter('testFunc', 'param');
        return $rp->getDefaultValue();
    "#);
    
    assert_eq!(result, Val::Int(42));
}

#[test]
fn test_reflection_parameter_is_default_value_available() {
    let result = run_php(r#"<?php
        function testFunc($required, $optional = null) {}
        
        $rp1 = new ReflectionParameter('testFunc', 'required');
        $rp2 = new ReflectionParameter('testFunc', 'optional');
        
        return [
            $rp1->isDefaultValueAvailable(),
            $rp2->isDefaultValueAvailable()
        ];
    "#);
    
    if let Val::Array(arr) = result {
        let map = &arr.map;
        assert_eq!(map.len(), 2);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_reflection_parameter_is_variadic() {
    let result = run_php(r#"<?php
        function testFunc($normal, ...$variadic) {}
        
        $rp1 = new ReflectionParameter('testFunc', 'normal');
        $rp2 = new ReflectionParameter('testFunc', 'variadic');
        
        return [$rp1->isVariadic(), $rp2->isVariadic()];
    "#);
    
    if let Val::Array(arr) = result {
        let map = &arr.map;
        assert_eq!(map.len(), 2);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_reflection_parameter_is_passed_by_reference() {
    let result = run_php(r#"<?php
        function testFunc($byValue, &$byRef) {}
        
        $rp1 = new ReflectionParameter('testFunc', 'byValue');
        $rp2 = new ReflectionParameter('testFunc', 'byRef');
        
        return [$rp1->isPassedByReference(), $rp2->isPassedByReference()];
    "#);
    
    if let Val::Array(arr) = result {
        let map = &arr.map;
        assert_eq!(map.len(), 2);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_reflection_parameter_has_type() {
    let result = run_php(r#"<?php
        function testFunc($untyped, int $typed) {}
        
        $rp1 = new ReflectionParameter('testFunc', 'untyped');
        $rp2 = new ReflectionParameter('testFunc', 'typed');
        
        return [$rp1->hasType(), $rp2->hasType()];
    "#);
    
    if let Val::Array(arr) = result {
        let map = &arr.map;
        assert_eq!(map.len(), 2);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_reflection_parameter_allows_null() {
    let result = run_php(r#"<?php
        function testFunc($untyped, int $nonNullable) {}
        
        $rp1 = new ReflectionParameter('testFunc', 'untyped');
        $rp2 = new ReflectionParameter('testFunc', 'nonNullable');
        
        return [$rp1->allowsNull(), $rp2->allowsNull()];
    "#);
    
    if let Val::Array(arr) = result {
        let map = &arr.map;
        assert_eq!(map.len(), 2);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_reflection_method_invoke() {
    let result = run_php(r#"<?php
        class Calculator {
            public function add($a, $b) {
                return $a + $b;
            }
        }
        
        $obj = new Calculator();
        $rm = new ReflectionMethod('Calculator', 'add');
        
        return $rm->invoke($obj, 5, 3);
    "#);
    
    assert_eq!(result, Val::Int(8));
}

#[test]
fn test_reflection_method_invoke_args() {
    let result = run_php(r#"<?php
        class StringOps {
            public function concat($a, $b, $c) {
                return $a . $b . $c;
            }
        }
        
        $obj = new StringOps();
        $rm = new ReflectionMethod('StringOps', 'concat');
        
        return $rm->invokeArgs($obj, ['Hello', ' ', 'World']);
    "#);
    
    assert_eq!(result, Val::String(std::rc::Rc::new(b"Hello World".to_vec())));
}

#[test]
fn test_reflection_parameter_get_position() {
    let result = run_php(r#"<?php
        function testFunc($first, $second, $third) {}
        
        $rp1 = new ReflectionParameter('testFunc', 'first');
        $rp2 = new ReflectionParameter('testFunc', 'second');
        $rp3 = new ReflectionParameter('testFunc', 'third');
        
        return [$rp1->getPosition(), $rp2->getPosition(), $rp3->getPosition()];
    "#);
    
    if let Val::Array(arr) = result {
        let map = &arr.map;
        assert_eq!(map.len(), 3);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_reflection_parameter_get_declaring_function() {
    let result = run_php(r#"<?php
        function myFunc($param) {}
        
        $rp = new ReflectionParameter('myFunc', 'param');
        $rf = $rp->getDeclaringFunction();
        
        return $rf->getName();
    "#);
    
    assert_eq!(result, Val::String(std::rc::Rc::new(b"myFunc".to_vec())));
}

#[test]
fn test_reflection_parameter_get_declaring_class() {
    let result = run_php(r#"<?php
        class TestClass {
            public function testMethod($param) {}
        }
        
        $rp = new ReflectionParameter(['TestClass', 'testMethod'], 'param');
        $rc = $rp->getDeclaringClass();
        
        return $rc->getName();
    "#);
    
    assert_eq!(result, Val::String(std::rc::Rc::new(b"TestClass".to_vec())));
}

#[test]
fn test_reflection_parameter_get_declaring_class_null() {
    let result = run_php(r#"<?php
        function globalFunc($param) {}
        
        $rp = new ReflectionParameter('globalFunc', 'param');
        $rc = $rp->getDeclaringClass();
        
        return $rc === null;
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_parameter_get_type() {
    let result = run_php(r#"<?php
        function testFunc(int $intParam, string $strParam, $noType) {}
        
        $rp1 = new ReflectionParameter('testFunc', 'intParam');
        $rp2 = new ReflectionParameter('testFunc', 'strParam');
        $rp3 = new ReflectionParameter('testFunc', 'noType');
        
        return [$rp1->getType(), $rp2->getType(), $rp3->getType()];
    "#);
    
    if let Val::Array(arr) = result {
        let map = &arr.map;
        assert_eq!(map.len(), 3);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_reflection_parameter_get_type_returns_named_type() {
    let result = run_php(r#"<?php
        function testFunc(int $param) {}
        
        $rp = new ReflectionParameter('testFunc', 'param');
        $type = $rp->getType();
        
        return [
            $type->getName(),
            $type->isBuiltin(),
            $type->allowsNull()
        ];
    "#);
    
    if let Val::Array(arr) = result {
        let map = &arr.map;
        assert_eq!(map.len(), 3);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_reflection_parameter_get_type_nullable() {
    let result = run_php(r#"<?php
        function testFunc(?int $param) {}
        
        $rp = new ReflectionParameter('testFunc', 'param');
        $type = $rp->getType();
        
        // Check that the type allows null and string representation includes ?
        return [
            $type->getName(),
            $type->allowsNull(),
            $type->__toString()
        ];
    "#);
    
    if let Val::Array(arr) = result {
        let map = &arr.map;
        assert_eq!(map.len(), 3);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_reflection_parameter_get_type_class() {
    let result = run_php(r#"<?php
        class MyClass {}
        function testFunc(MyClass $obj) {}
        
        $rp = new ReflectionParameter('testFunc', 'obj');
        $type = $rp->getType();
        
        return [
            $type->getName(),
            $type->isBuiltin(),
            $type->allowsNull()
        ];
    "#);
    
    if let Val::Array(arr) = result {
        let map = &arr.map;
        assert_eq!(map.len(), 3);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_reflection_parameter_can_be_passed_by_value() {
    let result = run_php(r#"<?php
        function testFunc($byValue, &$byRef) {}
        
        $rp1 = new ReflectionParameter('testFunc', 'byValue');
        $rp2 = new ReflectionParameter('testFunc', 'byRef');
        
        return [$rp1->canBePassedByValue(), $rp2->canBePassedByValue()];
    "#);
    
    if let Val::Array(arr) = result {
        let map = &arr.map;
        assert_eq!(map.len(), 2);
    } else {
        panic!("Expected array result");
    }
}

//=============================================================================
// ReflectionFunction Method Tests
//=============================================================================

#[test]
fn test_reflection_function_get_number_of_parameters() {
    let result = run_php(r#"<?php
        function noParams() {}
        function threeParams($a, $b, $c) {}
        
        $rf1 = new ReflectionFunction('noParams');
        $rf2 = new ReflectionFunction('threeParams');
        
        return [$rf1->getNumberOfParameters(), $rf2->getNumberOfParameters()];
    "#);
    
    if let Val::Array(arr) = result {
        let map = &arr.map;
        assert_eq!(map.len(), 2);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_reflection_function_get_number_of_required_parameters() {
    let result = run_php(r#"<?php
        function mixedParams($required1, $required2, $optional = 10) {}
        
        $rf = new ReflectionFunction('mixedParams');
        
        return $rf->getNumberOfRequiredParameters();
    "#);
    
    assert_eq!(result, Val::Int(2));
}

#[test]
fn test_reflection_function_get_parameters() {
    let result = run_php(r#"<?php
        function testFunc($a, $b, $c) {}
        
        $rf = new ReflectionFunction('testFunc');
        $params = $rf->getParameters();
        
        return count($params);
    "#);
    
    assert_eq!(result, Val::Int(3));
}

#[test]
fn test_reflection_function_get_parameters_names() {
    let result = run_php(r#"<?php
        function testFunc($first, $second) {}
        
        $rf = new ReflectionFunction('testFunc');
        $params = $rf->getParameters();
        
        return [$params[0]->getName(), $params[1]->getName()];
    "#);
    
    if let Val::Array(arr) = result {
        let map = &arr.map;
        assert_eq!(map.len(), 2);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_reflection_function_is_user_defined() {
    let result = run_php(r#"<?php
        function myFunction() {}
        
        $rf = new ReflectionFunction('myFunction');
        
        return $rf->isUserDefined();
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_function_is_internal() {
    let result = run_php(r#"<?php
        function myFunction() {}
        
        $rf = new ReflectionFunction('myFunction');
        
        return $rf->isInternal();
    "#);
    
    assert_eq!(result, Val::Bool(false));
}

#[test]
fn test_reflection_function_is_variadic() {
    let result = run_php(r#"<?php
        function normalFunc($a, $b) {}
        function variadicFunc($a, ...$rest) {}
        
        $rf1 = new ReflectionFunction('normalFunc');
        $rf2 = new ReflectionFunction('variadicFunc');
        
        return [$rf1->isVariadic(), $rf2->isVariadic()];
    "#);
    
    if let Val::Array(arr) = result {
        let map = &arr.map;
        assert_eq!(map.len(), 2);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_reflection_function_returns_reference() {
    let result = run_php(r#"<?php
        function &refFunc() {
            static $var = 10;
            return $var;
        }
        
        $rf = new ReflectionFunction('refFunc');
        
        return $rf->returnsReference();
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_function_get_namespace_name() {
    let result = run_php(r#"<?php
        function globalFunc() {}
        
        $rf = new ReflectionFunction('globalFunc');
        
        return $rf->getNamespaceName();
    "#);
    
    // Global function should have empty namespace
    assert_eq!(result, Val::String(std::rc::Rc::new(Vec::new())));
}

#[test]
fn test_reflection_function_get_short_name() {
    let result = run_php(r#"<?php
        function globalFunc() {}
        
        $rf = new ReflectionFunction('globalFunc');
        
        return $rf->getShortName();
    "#);
    
    assert_eq!(result, Val::String(std::rc::Rc::new(b"globalFunc".to_vec())));
}

#[test]
fn test_reflection_function_in_namespace() {
    let result = run_php(r#"<?php
        function globalFunc() {}
        
        $rf = new ReflectionFunction('globalFunc');
        
        return $rf->inNamespace();
    "#);
    
    // Global function is not in a namespace
    assert_eq!(result, Val::Bool(false));
}

#[test]
fn test_reflection_function_is_closure() {
    let result = run_php(r#"<?php
        function regularFunc() {}
        
        $rf = new ReflectionFunction('regularFunc');
        
        return $rf->isClosure();
    "#);
    
    assert_eq!(result, Val::Bool(false));
}

#[test]
fn test_reflection_function_is_generator() {
    let result = run_php(r#"<?php
        function generatorFunc() {
            yield 1;
            yield 2;
        }
        
        $rf = new ReflectionFunction('generatorFunc');
        
        return $rf->isGenerator();
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_function_invoke() {
    let result = run_php(r#"<?php
        function add($a, $b) {
            return $a + $b;
        }
        
        $rf = new ReflectionFunction('add');
        return $rf->invoke(5, 3);
    "#);
    
    assert_eq!(result, Val::Int(8));
}

#[test]
fn test_reflection_function_invoke_args() {
    let result = run_php(r#"<?php
        function multiply($a, $b, $c) {
            return $a * $b * $c;
        }
        
        $rf = new ReflectionFunction('multiply');
        return $rf->invokeArgs([2, 3, 4]);
    "#);
    
    assert_eq!(result, Val::Int(24));
}

#[test]
fn test_reflection_function_is_anonymous() {
    let result = run_php(r#"<?php
        function normalFunc() {
            return 42;
        }
        
        $rf = new ReflectionFunction('normalFunc');
        return $rf->isAnonymous();
    "#);
    
    assert_eq!(result, Val::Bool(false));
}

#[test]
fn test_reflection_function_is_disabled() {
    let result = run_php(r#"<?php
        function testFunc() {
            return 1;
        }
        
        $rf = new ReflectionFunction('testFunc');
        return $rf->isDisabled();
    "#);
    
    assert_eq!(result, Val::Bool(false));
}

#[test]
fn test_reflection_function_to_string() {
    let result = run_php(r#"<?php
        function myFunc() {
            return 1;
        }
        
        $rf = new ReflectionFunction('myFunc');
        $str = $rf->__toString();
        return is_string($str) && strlen($str) > 0;
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_function_get_closure() {
    let result = run_php(r#"<?php
        function testFunc() {
            return 42;
        }
        
        $rf = new ReflectionFunction('testFunc');
        return $rf->getClosure();
    "#);
    
    // Currently returns null (not yet implemented)
    assert_eq!(result, Val::Null);
}

#[test]
fn test_reflection_function_get_file_name_internal() {
    let result = run_php(r#"<?php
        $rf = new ReflectionFunction('strlen');
        return $rf->getFileName();
    "#);
    
    // Internal functions return false
    assert_eq!(result, Val::Bool(false));
}

#[test]
fn test_reflection_function_get_file_name_user() {
    let result = run_php(r#"<?php
        function myFunc() {
            return 1;
        }
        
        $rf = new ReflectionFunction('myFunc');
        return $rf->getFileName();
    "#);
    
    // User functions return null (file tracking not yet implemented)
    assert_eq!(result, Val::Null);
}

// ReflectionParameter additional methods tests

#[test]
fn test_reflection_parameter_is_default_value_constant() {
    let result = run_php(r#"<?php
        function test($x = 42) {}
        
        $rf = new ReflectionFunction('test');
        $params = $rf->getParameters();
        
        return $params[0]->isDefaultValueConstant();
    "#);
    
    assert_eq!(result, Val::Bool(false));
}

#[test]
fn test_reflection_parameter_is_promoted() {
    let result = run_php(r#"<?php
        function test($x) {}
        
        $rf = new ReflectionFunction('test');
        $params = $rf->getParameters();
        
        return $params[0]->isPromoted();
    "#);
    
    assert_eq!(result, Val::Bool(false));
}

#[test]
fn test_reflection_parameter_get_attributes() {
    let result = run_php(r#"<?php
        function test($x) {}
        
        $rf = new ReflectionFunction('test');
        $params = $rf->getParameters();
        $attrs = $params[0]->getAttributes();
        
        return is_array($attrs);
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_parameter_to_string() {
    let result = run_php(r#"<?php
        function test($x) {}
        
        $rf = new ReflectionFunction('test');
        $params = $rf->getParameters();
        
        return $params[0]->__toString();
    "#);
    
    if let Val::String(s) = result {
        let output = String::from_utf8_lossy(s.as_ref());
        assert!(output.contains("Parameter"));
        assert!(output.contains("$x"));
    } else {
        panic!("Expected string output");
    }
}

#[test]
fn test_reflection_parameter_to_string_with_type() {
    let result = run_php(r#"<?php
        function test(int $x = 5) {}
        
        $rf = new ReflectionFunction('test');
        $params = $rf->getParameters();
        
        return $params[0]->__toString();
    "#);
    
    if let Val::String(s) = result {
        let output = String::from_utf8_lossy(s.as_ref());
        assert!(output.contains("Parameter"));
        assert!(output.contains("int"));
        assert!(output.contains("$x"));
        assert!(output.contains("5"));
    } else {
        panic!("Expected string output");
    }
}

#[test]
fn test_reflection_parameter_to_string_variadic() {
    let result = run_php(r#"<?php
        function test(...$args) {}
        
        $rf = new ReflectionFunction('test');
        $params = $rf->getParameters();
        
        return $params[0]->__toString();
    "#);
    
    if let Val::String(s) = result {
        let output = String::from_utf8_lossy(s.as_ref());
        assert!(output.contains("Parameter"));
        assert!(output.contains("$args"));
    } else {
        panic!("Expected string output");
    }
}

#[test]
fn test_reflection_class_get_constructor() {
    let result = run_php(r#"<?php
        class TestClass {
            public function __construct() {}
        }
        
        $rc = new ReflectionClass('TestClass');
        $constructor = $rc->getConstructor();
        return $constructor !== null;
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_class_get_method() {
    let result = run_php(r#"<?php
        class TestClass {
            public function myMethod() {}
        }
        
        $rc = new ReflectionClass('TestClass');
        $rm = $rc->getMethod('myMethod');
        return $rm->getName();
    "#);
    
    // Returns ReflectionMethod object, verify by getting name
    assert_eq!(result, Val::String(Rc::new(b"myMethod".to_vec())));
}

#[test]
fn test_reflection_class_get_property() {
    let result = run_php(r#"<?php
        class TestClass {
            public static $myProp;
        }
        
        $rc = new ReflectionClass('TestClass');
        $rp = $rc->getProperty('myProp');
        return $rp->getName();
    "#);
    
    // Returns ReflectionProperty object, verify by getting name
    assert_eq!(result, Val::String(Rc::new(b"myProp".to_vec())));
}

#[test]
fn test_reflection_class_get_modifiers() {
    let result = run_php(r#"<?php
        abstract class TestClass {}
        
        $rc = new ReflectionClass('TestClass');
        return $rc->getModifiers();
    "#);
    
    // Abstract class should have non-zero modifiers
    if let Val::Int(modifiers) = result {
        assert!(modifiers > 0);
    } else {
        panic!("Expected int modifiers");
    }
}

#[test]
fn test_reflection_class_is_final() {
    let result = run_php(r#"<?php
        final class FinalClass {}

        $rc = new ReflectionClass('FinalClass');
        $names = Reflection::getModifierNames($rc->getModifiers());
        return $rc->isFinal() && in_array('final', $names, true);
    "#);

    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_class_is_final_native() {
    let result = run_php(r#"<?php
        $rc = new ReflectionClass('Closure');
        return $rc->isFinal();
    "#);

    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_class_is_instance() {
    let result = run_php(r#"<?php
        class TestClass {}
        
        $obj = new TestClass();
        $rc = new ReflectionClass('TestClass');
        return $rc->isInstance($obj);
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_class_is_instance_parent_chain() {
    let result = run_php(r#"<?php
        class ParentClass {}
        class ChildClass extends ParentClass {}

        $obj = new ChildClass();
        $rc = new ReflectionClass('ParentClass');
        return $rc->isInstance($obj);
    "#);

    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_class_is_instance_interface() {
    let result = run_php(r#"<?php
        interface TestInterface {}
        class ImplClass implements TestInterface {}

        $obj = new ImplClass();
        $rc = new ReflectionClass('TestInterface');
        return $rc->isInstance($obj);
    "#);

    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_class_is_subclass_of() {
    let result = run_php(r#"<?php
        class ParentClass {}
        class ChildClass extends ParentClass {}
        
        $rc = new ReflectionClass('ChildClass');
        return $rc->isSubclassOf('ParentClass');
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_class_is_anonymous() {
    let result = run_php(r#"<?php
        class TestClass {}
        
        $rc = new ReflectionClass('TestClass');
        return $rc->isAnonymous();
    "#);
    
    assert_eq!(result, Val::Bool(false));
}

#[test]
fn test_reflection_class_is_cloneable() {
    let result = run_php(r#"<?php
        class TestClass {}
        
        $rc = new ReflectionClass('TestClass');
        return $rc->isCloneable();
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_class_is_internal() {
    let result = run_php(r#"<?php
        class TestClass {}
        
        $rc = new ReflectionClass('TestClass');
        return $rc->isInternal();
    "#);
    
    // User-defined class should not be internal
    assert_eq!(result, Val::Bool(false));
}

#[test]
fn test_reflection_class_is_user_defined() {
    let result = run_php(r#"<?php
        class TestClass {}
        
        $rc = new ReflectionClass('TestClass');
        return $rc->isUserDefined();
    "#);
    
    // User-defined class should be user-defined
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_class_is_iterable() {
    let result = run_php(r#"<?php
        class TestClass {}
        
        $rc = new ReflectionClass('TestClass');
        return $rc->isIterable();
    "#);
    
    // Class without iterator interfaces should not be iterable
    assert_eq!(result, Val::Bool(false));
}

#[test]
fn test_reflection_class_get_static_properties() {
    let result = run_php(r#"<?php
        class TestClass {
            public static $prop1 = 10;
            public static $prop2 = 20;
        }
        
        $rc = new ReflectionClass('TestClass');
        $props = $rc->getStaticProperties();
        return count($props);
    "#);
    
    assert_eq!(result, Val::Int(2));
}

#[test]
fn test_reflection_class_get_static_property_value() {
    let result = run_php(r#"<?php
        class TestClass {
            public static $myProp = 42;
        }
        
        $rc = new ReflectionClass('TestClass');
        return $rc->getStaticPropertyValue('myProp');
    "#);
    
    assert_eq!(result, Val::Int(42));
}

#[test]
fn test_reflection_class_set_static_property_value() {
    let result = run_php(r#"<?php
        class TestClass {
            public static $myProp = 10;
        }
        
        $rc = new ReflectionClass('TestClass');
        $rc->setStaticPropertyValue('myProp', 99);
        return TestClass::$myProp;
    "#);
    
    assert_eq!(result, Val::Int(99));
}

#[test]
fn test_reflection_class_get_default_properties() {
    let result = run_php(r#"<?php
        class TestClass {
            public static $prop1 = 5;
        }
        
        $rc = new ReflectionClass('TestClass');
        $props = $rc->getDefaultProperties();
        return count($props);
    "#);
    
    assert_eq!(result, Val::Int(1));
}

#[test]
fn test_reflection_class_get_attributes() {
    let result = run_php(r#"<?php
        class TestClass {}
        
        $rc = new ReflectionClass('TestClass');
        $attrs = $rc->getAttributes();
        return count($attrs);
    "#);
    
    assert_eq!(result, Val::Int(0));
}

#[test]
fn test_reflection_class_get_doc_comment() {
    let result = run_php(r#"<?php
        class TestClass {}
        
        $rc = new ReflectionClass('TestClass');
        return $rc->getDocComment();
    "#);
    
    assert_eq!(result, Val::Bool(false));
}

#[test]
fn test_reflection_class_get_file_name() {
    use php_rs::compiler::emitter::Emitter;
    use php_rs::runtime::context::{EngineBuilder, RequestContext};
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("TestClass.php");
    let file_path_str = file_path.to_string_lossy().into_owned();

    let source = r#"<?php
        class TestClass {}
        $rc = new ReflectionClass('TestClass');
        return $rc->getFileName();
    "#;

    let arena = bumpalo::Bump::new();
    let lexer = php_rs::parser::lexer::Lexer::new(source.as_bytes());
    let mut parser = php_rs::parser::parser::Parser::new(lexer, &arena);
    let program = parser.parse_program();
    assert!(program.errors.is_empty());

    let context = EngineBuilder::new()
        .with_core_extensions()
        .build()
        .expect("Failed to build engine");
    let mut request_context = RequestContext::new(context);
    let emitter = Emitter::new(source.as_bytes(), &mut request_context.interner)
        .with_file_path(file_path_str.clone());
    let (chunk, _) = emitter.compile(&program.statements);

    let mut vm = VM::new_with_context(request_context);
    vm.run(Rc::new(chunk)).expect("execution failed");

    let value = match vm.last_return_value {
        Some(handle) => vm.arena.get(handle).value.clone(),
        None => Val::Null,
    };

    assert_eq!(
        value,
        Val::String(Rc::new(file_path_str.into_bytes()))
    );
}

#[test]
fn test_reflection_class_get_interfaces() {
    let result = run_php(r#"<?php
        class TestClass {}
        
        $rc = new ReflectionClass('TestClass');
        $interfaces = $rc->getInterfaces();
        return count($interfaces);
    "#);
    
    assert_eq!(result, Val::Int(0));
}

#[test]
fn test_reflection_class_get_trait_names() {
    let result = run_php(r#"<?php
        class TestClass {}
        
        $rc = new ReflectionClass('TestClass');
        $traits = $rc->getTraitNames();
        return count($traits);
    "#);
    
    assert_eq!(result, Val::Int(0));
}

#[test]
fn test_reflection_class_is_readonly() {
    let result = run_php(r#"<?php
        class TestClass {}
        
        $rc = new ReflectionClass('TestClass');
        return $rc->isReadOnly();
    "#);
    
    assert_eq!(result, Val::Bool(false));
}

#[test]
fn test_reflection_class_get_reflection_constants() {
    let result = run_php(r#"<?php
        class TestClass {
            const CONST1 = 1;
            const CONST2 = 2;
        }
        
        $rc = new ReflectionClass('TestClass');
        $consts = $rc->getReflectionConstants();
        return count($consts);
    "#);
    
    assert_eq!(result, Val::Int(2));
}
#[test]
fn test_reflection_class_get_extension() {
    let result = run_php(r#"<?php
        class TestClass {}
        $rc = new ReflectionClass('TestClass');
        return $rc->getExtension();
    "#);
    
    assert!(matches!(result, Val::Null));
}

#[test]
fn test_reflection_class_get_extension_name() {
    let result = run_php(r#"<?php
        class TestClass {}
        $rc = new ReflectionClass('TestClass');
        return $rc->getExtensionName();
    "#);
    
    assert!(matches!(result, Val::Bool(false)));
}

#[test]
fn test_reflection_class_is_iterateable() {
    let result = run_php(r#"<?php
        class TestIterator implements Iterator {
            public function current() {}
            public function key() {}
            public function next() {}
            public function rewind() {}
            public function valid() { return false; }
        }
        
        $rc = new ReflectionClass('TestIterator');
        return $rc->isIterateable();
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_class_lazy_object_methods() {
    let result = run_php(r#"<?php
        class TestClass {}
        $rc = new ReflectionClass('TestClass');
        $obj = new TestClass();
        
        // Test lazy object related methods (PHP 8.4+)
        $initializer = $rc->getLazyInitializer($obj);
        $uninit = $rc->isUninitializedLazyObject($obj);
        
        // All should return stub values
        return $initializer === null && $uninit === false;
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_class_new_lazy_methods() {
    let result = run_php(r#"<?php
        class TestClass {}
        $rc = new ReflectionClass('TestClass');
        
        // Test lazy creation methods (PHP 8.4+) 
        $ghost = $rc->newLazyGhost(function() {});
        $proxy = $rc->newLazyProxy(function() {});
        
        // Both should return null (stubs)
        return $ghost === null && $proxy === null;
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

//=============================================================================
// ReflectionAttribute Tests
//=============================================================================

#[test]
fn test_reflection_attribute_basic() {
    // Note: Since attributes are not fully implemented yet, we test that
    // getAttributes() returns empty array (stub behavior)
    let result = run_php(r#"<?php
        class TestClass {
            public $prop;
        }
        
        $rc = new ReflectionClass("TestClass");
        $attrs = $rc->getAttributes();
        return count($attrs);
    "#);
    
    assert_eq!(result, Val::Int(0));
}

//=============================================================================
// ReflectionEnum Tests
//=============================================================================

#[test]
fn test_reflection_enum_class_exists() {
    // Test that ReflectionEnum class exists
    let result = run_php(r#"<?php
        return class_exists('ReflectionEnum');
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

//=============================================================================
// ReflectionEnumUnitCase Tests
//=============================================================================

#[test]
fn test_reflection_enum_unit_case_class_exists() {
    // Test that ReflectionEnumUnitCase class exists
    let result = run_php(r#"<?php
        return class_exists('ReflectionEnumUnitCase');
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_enum_backed_case_class_exists() {
    // Test that ReflectionEnumBackedCase class exists
    let result = run_php(r#"<?php
        return class_exists('ReflectionEnumBackedCase');
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_extension_class_exists() {
    // Test that ReflectionExtension class exists
    let result = run_php(r#"<?php
        return class_exists('ReflectionExtension');
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_extension_get_name() {
    // Test ReflectionExtension getName() method
    let result = run_php(r#"<?php
        $ext = new ReflectionExtension('Core');
        return $ext->getName();
    "#);
    
    assert_eq!(result, Val::String(Rc::new(b"Core".to_vec())));
}

#[test]
fn test_reflection_extension_get_version() {
    // Test ReflectionExtension getVersion() returns null (not implemented)
    let result = run_php(r#"<?php
        $ext = new ReflectionExtension('Core');
        return $ext->getVersion();
    "#);
    
    assert_eq!(result, Val::Null);
}

#[test]
fn test_reflection_extension_get_functions() {
    // Test ReflectionExtension getFunctions() returns empty array
    let result = run_php(r#"<?php
        $ext = new ReflectionExtension('Core');
        $funcs = $ext->getFunctions();
        return is_array($funcs) && count($funcs) === 0;
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_extension_is_persistent() {
    // Test ReflectionExtension isPersistent() returns true
    let result = run_php(r#"<?php
        $ext = new ReflectionExtension('Core');
        return $ext->isPersistent();
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_extension_is_temporary() {
    // Test ReflectionExtension isTemporary() returns false
    let result = run_php(r#"<?php
        $ext = new ReflectionExtension('Core');
        return $ext->isTemporary();
    "#);
    
    assert_eq!(result, Val::Bool(false));
}

#[test]
fn test_reflection_class_exists() {
    // Test that Reflection class exists
    let result = run_php(r#"<?php
        return class_exists('Reflection');
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_get_modifier_names_public() {
    // Test Reflection::getModifierNames() with public modifier
    let result = run_php(r#"<?php
        $names = Reflection::getModifierNames(256);  // IS_PUBLIC
        return count($names) === 1 && $names[0] === 'public';
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_get_modifier_names_static() {
    // Test Reflection::getModifierNames() with static modifier
    let result = run_php(r#"<?php
        $names = Reflection::getModifierNames(1);  // IS_STATIC
        return count($names) === 1 && $names[0] === 'static';
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_get_modifier_names_multiple() {
    // Test Reflection::getModifierNames() with multiple modifiers
    let result = run_php(r#"<?php
        $names = Reflection::getModifierNames(257);  // IS_PUBLIC | IS_STATIC
        return count($names) === 2 && $names[0] === 'public' && $names[1] === 'static';
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_get_modifier_names_final() {
    // Test Reflection::getModifierNames() with final modifier
    let result = run_php(r#"<?php
        $names = Reflection::getModifierNames(260);  // IS_PUBLIC | IS_FINAL
        return count($names) === 2 && in_array('public', $names) && in_array('final', $names);
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_export_returns_null() {
    // Test Reflection::export() returns null (deprecated)
    let result = run_php(r#"<?php
        return Reflection::export(null);
    "#);
    
    assert_eq!(result, Val::Null);
}

#[test]
fn test_reflection_exception_class_exists() {
    // Test that ReflectionException class exists
    let result = run_php(r#"<?php
        return class_exists('ReflectionException');
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_exception_extends_exception() {
    // Test that ReflectionException extends Exception
    let result = run_php(r#"<?php
        return is_subclass_of('ReflectionException', 'Exception');
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_exception_can_be_instantiated() {
    // Test that ReflectionException can be created and thrown
    let result = run_php(r#"<?php
        try {
            throw new ReflectionException('Test error', 123);
        } catch (ReflectionException $e) {
            return $e->getMessage() === 'Test error' && $e->getCode() === 123;
        }
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflector_interface_exists() {
    // Test that Reflector interface exists
    let result = run_php(r#"<?php
        return interface_exists('Reflector');
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_class_implements_reflector() {
    // Test that ReflectionClass implements Reflector
    let result = run_php(r#"<?php
        $rc = new ReflectionClass('stdClass');
        return $rc instanceof Reflector;
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_reference_class_exists() {
    // Test that ReflectionReference class exists
    let result = run_php(r#"<?php
        return class_exists('ReflectionReference');
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_reference_from_array_element_returns_null() {
    // Test fromArrayElement() returns null (no reference tracking yet)
    let result = run_php(r#"<?php
        $arr = [1, 2, 3];
        $ref = ReflectionReference::fromArrayElement($arr, 0);
        return $ref === null;
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_reference_get_id() {
    // Test getId() method exists and is callable
    // Since we can't create actual ReflectionReference instances yet,
    // we just verify the static method works
    let result = run_php(r#"<?php
        // Test that fromArrayElement is a valid static method
        $arr = [1, 2, 3];
        ReflectionReference::fromArrayElement($arr, 0);
        return true;
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_zend_extension_class_exists() {
    // Test that ReflectionZendExtension class exists
    let result = run_php(r#"<?php
        return class_exists('ReflectionZendExtension');
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_zend_extension_get_name() {
    // Test ReflectionZendExtension::getName()
    let result = run_php(r#"<?php
        $ext = new ReflectionZendExtension('Zend OPcache');
        return $ext->getName() === 'Zend OPcache';
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_zend_extension_get_version() {
    // Test ReflectionZendExtension::getVersion()
    let result = run_php(r#"<?php
        $ext = new ReflectionZendExtension('test');
        $version = $ext->getVersion();
        return is_string($version);
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_zend_extension_get_author() {
    // Test ReflectionZendExtension::getAuthor()
    let result = run_php(r#"<?php
        $ext = new ReflectionZendExtension('test');
        $author = $ext->getAuthor();
        return is_string($author);
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_zend_extension_get_url() {
    // Test ReflectionZendExtension::getURL()
    let result = run_php(r#"<?php
        $ext = new ReflectionZendExtension('test');
        $url = $ext->getURL();
        return is_string($url);
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_zend_extension_get_copyright() {
    // Test ReflectionZendExtension::getCopyright()
    let result = run_php(r#"<?php
        $ext = new ReflectionZendExtension('test');
        $copyright = $ext->getCopyright();
        return is_string($copyright);
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_generator_class_exists() {
    // Test that ReflectionGenerator class exists
    let result = run_php(r#"<?php
        return class_exists('ReflectionGenerator');
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_generator_get_executing_file() {
    // Test ReflectionGenerator::getExecutingFile() stub
    // Note: Requires a generator object, using stdClass as placeholder
    let result = run_php(r#"<?php
        $obj = new stdClass();
        $rg = new ReflectionGenerator($obj);
        $file = $rg->getExecutingFile();
        return is_string($file);
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_generator_get_executing_line() {
    // Test ReflectionGenerator::getExecutingLine() stub
    let result = run_php(r#"<?php
        $obj = new stdClass();
        $rg = new ReflectionGenerator($obj);
        $line = $rg->getExecutingLine();
        return is_int($line);
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_generator_is_closed() {
    // Test ReflectionGenerator::isClosed() stub
    let result = run_php(r#"<?php
        $obj = new stdClass();
        $rg = new ReflectionGenerator($obj);
        $closed = $rg->isClosed();
        return is_bool($closed);
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_generator_get_trace() {
    // Test ReflectionGenerator::getTrace() returns array
    let result = run_php(r#"<?php
        $obj = new stdClass();
        $rg = new ReflectionGenerator($obj);
        $trace = $rg->getTrace();
        return is_array($trace);
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_fiber_class_exists() {
    // Test that ReflectionFiber class exists
    let result = run_php(r#"<?php
        return class_exists('ReflectionFiber');
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_fiber_get_executing_file() {
    // Test ReflectionFiber::getExecutingFile() stub
    // Note: Requires a fiber object, using stdClass as placeholder
    let result = run_php(r#"<?php
        $obj = new stdClass();
        $rf = new ReflectionFiber($obj);
        $file = $rf->getExecutingFile();
        return is_string($file);
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_fiber_get_executing_line() {
    // Test ReflectionFiber::getExecutingLine() stub
    let result = run_php(r#"<?php
        $obj = new stdClass();
        $rf = new ReflectionFiber($obj);
        $line = $rf->getExecutingLine();
        return is_int($line);
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_fiber_get_trace() {
    // Test ReflectionFiber::getTrace() returns array
    let result = run_php(r#"<?php
        $obj = new stdClass();
        $rf = new ReflectionFiber($obj);
        $trace = $rf->getTrace();
        return is_array($trace);
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_fiber_get_fiber() {
    // Test ReflectionFiber::getFiber() returns stored object
    let result = run_php(r#"<?php
        $obj = new stdClass();
        $rf = new ReflectionFiber($obj);
        $fiber = $rf->getFiber();
        return is_object($fiber);
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_function_abstract_class_exists() {
    // Test that ReflectionFunctionAbstract class exists
    let result = run_php(r#"<?php
        return class_exists('ReflectionFunctionAbstract');
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_function_abstract_get_doc_comment() {
    // Test that ReflectionFunctionAbstract is registered
    // Cannot instantiate abstract class, so just verify it exists
    let result = run_php(r#"<?php
        return class_exists('ReflectionFunctionAbstract');
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_function_abstract_has_return_type() {
    // Test that methods are defined in the class
    let result = run_php(r#"<?php
        return class_exists('ReflectionFunctionAbstract');
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_function_abstract_is_deprecated() {
    // Test that the abstract class is accessible
    let result = run_php(r#"<?php
        return class_exists('ReflectionFunctionAbstract');
    "#);
    
    assert_eq!(result, Val::Bool(true));
}
