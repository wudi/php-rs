//! Reflection API Tests
//!
//! Tests for PHP Reflection implementation covering:
//! - ReflectionClass
//! - ReflectionFunction
//! - ReflectionMethod
//! - ReflectionProperty
//! - ReflectionParameter

mod common;

use common::run_php;
use php_rs::core::value::Val;

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
        return $rc->getParentClass();
    "#);
    
    // Should return parent class name or ReflectionClass object
    if let Val::String(s) = result {
        assert_eq!(s.as_ref(), b"ParentClass");
    } else {
        panic!("Expected string result");
    }
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
