//! ReflectionClassConstant Tests
//!
//! Tests for ReflectionClassConstant class

mod common;

use common::run_php;
use php_rs::core::value::Val;

#[test]
fn test_reflection_class_constant_basic() {
    let result = run_php(r#"<?php
        class MyClass {
            const MY_CONST = 42;
        }
        
        $rc = new ReflectionClassConstant('MyClass', 'MY_CONST');
        
        return $rc->getValue();
    "#);
    
    assert_eq!(result, Val::Int(42));
}

#[test]
fn test_reflection_class_constant_get_name() {
    let result = run_php(r#"<?php
        class TestClass {
            const TEST_CONSTANT = "value";
        }
        
        $rc = new ReflectionClassConstant('TestClass', 'TEST_CONSTANT');
        
        return $rc->getName();
    "#);
    
    assert_eq!(result, Val::String(std::rc::Rc::new(b"TEST_CONSTANT".to_vec())));
}

#[test]
fn test_reflection_class_constant_get_value() {
    let result = run_php(r#"<?php
        class Values {
            const STRING_VALUE = "hello";
            const INT_VALUE = 123;
            const BOOL_VALUE = true;
        }
        
        $rc1 = new ReflectionClassConstant('Values', 'STRING_VALUE');
        $rc2 = new ReflectionClassConstant('Values', 'INT_VALUE');
        $rc3 = new ReflectionClassConstant('Values', 'BOOL_VALUE');
        
        return [
            $rc1->getValue(),
            $rc2->getValue(),
            $rc3->getValue()
        ];
    "#);
    
    if let Val::Array(arr) = result {
        assert_eq!(arr.map.len(), 3);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_reflection_class_constant_is_public() {
    let result = run_php(r#"<?php
        class ConstClass {
            public const PUBLIC_CONST = 1;
            private const PRIVATE_CONST = 2;
            protected const PROTECTED_CONST = 3;
        }
        
        $rc1 = new ReflectionClassConstant('ConstClass', 'PUBLIC_CONST');
        $rc2 = new ReflectionClassConstant('ConstClass', 'PRIVATE_CONST');
        $rc3 = new ReflectionClassConstant('ConstClass', 'PROTECTED_CONST');
        
        return [
            $rc1->isPublic(),
            $rc2->isPublic(),
            $rc3->isPublic()
        ];
    "#);
    
    if let Val::Array(arr) = result {
        assert_eq!(arr.map.len(), 3);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_reflection_class_constant_is_private() {
    let result = run_php(r#"<?php
        class ConstClass {
            public const PUBLIC_CONST = 1;
            private const PRIVATE_CONST = 2;
        }
        
        $rc1 = new ReflectionClassConstant('ConstClass', 'PUBLIC_CONST');
        $rc2 = new ReflectionClassConstant('ConstClass', 'PRIVATE_CONST');
        
        return [
            $rc1->isPrivate(),
            $rc2->isPrivate()
        ];
    "#);
    
    if let Val::Array(arr) = result {
        assert_eq!(arr.map.len(), 2);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_reflection_class_constant_is_protected() {
    let result = run_php(r#"<?php
        class ConstClass {
            public const PUBLIC_CONST = 1;
            protected const PROTECTED_CONST = 2;
        }
        
        $rc1 = new ReflectionClassConstant('ConstClass', 'PUBLIC_CONST');
        $rc2 = new ReflectionClassConstant('ConstClass', 'PROTECTED_CONST');
        
        return [
            $rc1->isProtected(),
            $rc2->isProtected()
        ];
    "#);
    
    if let Val::Array(arr) = result {
        assert_eq!(arr.map.len(), 2);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_reflection_class_constant_get_modifiers() {
    let result = run_php(r#"<?php
        class ConstClass {
            public const PUBLIC_CONST = 1;
            private const PRIVATE_CONST = 2;
            protected const PROTECTED_CONST = 3;
        }
        
        $rc1 = new ReflectionClassConstant('ConstClass', 'PUBLIC_CONST');
        $rc2 = new ReflectionClassConstant('ConstClass', 'PRIVATE_CONST');
        $rc3 = new ReflectionClassConstant('ConstClass', 'PROTECTED_CONST');
        
        return [
            $rc1->getModifiers(),
            $rc2->getModifiers(),
            $rc3->getModifiers()
        ];
    "#);
    
    if let Val::Array(arr) = result {
        assert_eq!(arr.map.len(), 3);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_reflection_class_constant_get_declaring_class() {
    let result = run_php(r#"<?php
        class MyClass {
            const MY_CONST = 100;
        }
        
        $rc = new ReflectionClassConstant('MyClass', 'MY_CONST');
        $class = $rc->getDeclaringClass();
        
        return $class->getName();
    "#);
    
    assert_eq!(result, Val::String(std::rc::Rc::new(b"MyClass".to_vec())));
}

#[test]
fn test_reflection_class_constant_with_object() {
    let result = run_php(r#"<?php
        class TestClass {
            const VALUE = 999;
        }
        
        $obj = new TestClass();
        $rc = new ReflectionClassConstant($obj, 'VALUE');
        
        return $rc->getValue();
    "#);
    
    assert_eq!(result, Val::Int(999));
}

#[test]
fn test_reflection_class_constant_to_string() {
    let result = run_php(r#"<?php
        class StringTest {
            public const MY_CONST = 42;
        }
        
        $rc = new ReflectionClassConstant('StringTest', 'MY_CONST');
        $str = $rc->__toString();
        
        return strlen($str) > 0;
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

#[test]
#[should_panic(expected = "does not exist")]
fn test_reflection_class_constant_nonexistent() {
    run_php(r#"<?php
        class TestClass {
            const EXISTS = 1;
        }
        
        // This should throw an error
        $rc = new ReflectionClassConstant('TestClass', 'DOES_NOT_EXIST');
    "#);
}

#[test]
fn test_reflection_class_constant_multiple_constants() {
    let result = run_php(r#"<?php
        class MultiConst {
            const CONST1 = "a";
            const CONST2 = "b";
            const CONST3 = "c";
        }
        
        $rc1 = new ReflectionClassConstant('MultiConst', 'CONST1');
        $rc2 = new ReflectionClassConstant('MultiConst', 'CONST2');
        $rc3 = new ReflectionClassConstant('MultiConst', 'CONST3');
        
        return [
            $rc1->getName(),
            $rc2->getName(),
            $rc3->getName()
        ];
    "#);
    
    if let Val::Array(arr) = result {
        assert_eq!(arr.map.len(), 3);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_reflection_class_constant_value_types() {
    let result = run_php(r#"<?php
        class TypedConsts {
            const STR = "string";
            const NUM = 42;
            const FLOAT = 3.14;
            const BOOL_TRUE = true;
            const BOOL_FALSE = false;
        }
        
        $rcs = new ReflectionClassConstant('TypedConsts', 'STR');
        $rcn = new ReflectionClassConstant('TypedConsts', 'NUM');
        $rcf = new ReflectionClassConstant('TypedConsts', 'FLOAT');
        $rct = new ReflectionClassConstant('TypedConsts', 'BOOL_TRUE');
        $rcff = new ReflectionClassConstant('TypedConsts', 'BOOL_FALSE');
        
        return [
            $rcs->getValue(),
            $rcn->getValue(),
            $rcf->getValue(),
            $rct->getValue(),
            $rcff->getValue()
        ];
    "#);
    
    if let Val::Array(arr) = result {
        assert_eq!(arr.map.len(), 5);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_reflection_class_constant_get_attributes() {
    let result = run_php(r#"<?php
        class MyClass {
            const MY_CONST = 'value';
        }
        
        $rc = new ReflectionClassConstant('MyClass', 'MY_CONST');
        return $rc->getAttributes();
    "#);
    
    // Should return empty array (attributes not yet implemented)
    if let Val::Array(arr) = result {
        assert_eq!(arr.map.len(), 0);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_reflection_class_constant_get_doc_comment() {
    let result = run_php(r#"<?php
        class MyClass {
            const MY_CONST = 'value';
        }
        
        $rc = new ReflectionClassConstant('MyClass', 'MY_CONST');
        return $rc->getDocComment();
    "#);
    
    // Should return false (doc comments not yet tracked)
    assert_eq!(result, Val::Bool(false));
}

#[test]
fn test_reflection_class_constant_has_type() {
    let result = run_php(r#"<?php
        class MyClass {
            const MY_CONST = 'value';
        }
        
        $rc = new ReflectionClassConstant('MyClass', 'MY_CONST');
        return $rc->hasType();
    "#);
    
    // Should return false (typed constants not yet implemented)
    assert_eq!(result, Val::Bool(false));
}

#[test]
fn test_reflection_class_constant_get_type() {
    let result = run_php(r#"<?php
        class MyClass {
            const MY_CONST = 'value';
        }
        
        $rc = new ReflectionClassConstant('MyClass', 'MY_CONST');
        return $rc->getType();
    "#);
    
    // Should return null (typed constants not yet implemented)
    assert_eq!(result, Val::Null);
}

#[test]
fn test_reflection_class_constant_is_enum_case() {
    let result = run_php(r#"<?php
        class MyClass {
            const MY_CONST = 'value';
        }
        
        $rc = new ReflectionClassConstant('MyClass', 'MY_CONST');
        return $rc->isEnumCase();
    "#);
    
    // Should return false (class is not an enum)
    assert_eq!(result, Val::Bool(false));
}

#[test]
fn test_reflection_class_constant_is_final() {
    let result = run_php(r#"<?php
        class MyClass {
            const MY_CONST = 'value';
        }
        
        $rc = new ReflectionClassConstant('MyClass', 'MY_CONST');
        return $rc->isFinal();
    "#);
    
    // Should return false (final constants not yet tracked)
    assert_eq!(result, Val::Bool(false));
}

#[test]
fn test_reflection_class_constant_is_deprecated() {
    let result = run_php(r#"<?php
        class MyClass {
            const MY_CONST = 'value';
        }
        
        $rc = new ReflectionClassConstant('MyClass', 'MY_CONST');
        return $rc->isDeprecated();
    "#);
    
    // Should return false
    assert_eq!(result, Val::Bool(false));
}
