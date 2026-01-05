mod common;

use common::run_php;
use php_rs::core::value::Val;

#[test]
fn test_reflection_property_construct_and_get_name() {
    let result = run_php(
        r#"<?php
        class TestClass {
            public $publicProp;
            private $privateProp;
            protected $protectedProp;
        }
        
        $prop = new ReflectionProperty('TestClass', 'publicProp');
        return $prop->getName();
        "#,
    );
    assert_eq!(result, Val::String(std::rc::Rc::new(b"publicProp".to_vec())));
}

#[test]
fn test_reflection_property_construct_with_object() {
    let result = run_php(
        r#"<?php
        class TestClass {
            public $value = 42;
        }
        
        $obj = new TestClass();
        $prop = new ReflectionProperty($obj, 'value');
        return $prop->getName();
        "#,
    );
    assert_eq!(result, Val::String(std::rc::Rc::new(b"value".to_vec())));
}

#[test]
fn test_reflection_property_is_public() {
    let result = run_php(
        r#"<?php
        class TestClass {
            public $publicProp;
            private $privateProp;
            protected $protectedProp;
        }
        
        $prop1 = new ReflectionProperty('TestClass', 'publicProp');
        $prop2 = new ReflectionProperty('TestClass', 'privateProp');
        $prop3 = new ReflectionProperty('TestClass', 'protectedProp');
        
        return [
            $prop1->isPublic(),
            $prop2->isPublic(),
            $prop3->isPublic()
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
fn test_reflection_property_is_private() {
    let result = run_php(
        r#"<?php
        class TestClass {
            public $publicProp;
            private $privateProp;
            protected $protectedProp;
        }
        
        $prop1 = new ReflectionProperty('TestClass', 'publicProp');
        $prop2 = new ReflectionProperty('TestClass', 'privateProp');
        $prop3 = new ReflectionProperty('TestClass', 'protectedProp');
        
        return [
            $prop1->isPrivate(),
            $prop2->isPrivate(),
            $prop3->isPrivate()
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
fn test_reflection_property_is_protected() {
    let result = run_php(
        r#"<?php
        class TestClass {
            public $publicProp;
            private $privateProp;
            protected $protectedProp;
        }
        
        $prop1 = new ReflectionProperty('TestClass', 'publicProp');
        $prop2 = new ReflectionProperty('TestClass', 'privateProp');
        $prop3 = new ReflectionProperty('TestClass', 'protectedProp');
        
        return [
            $prop1->isProtected(),
            $prop2->isProtected(),
            $prop3->isProtected()
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
fn test_reflection_property_is_static() {
    let result = run_php(
        r#"<?php
        class TestClass {
            public $instanceProp;
            public static $staticProp;
        }
        
        $prop1 = new ReflectionProperty('TestClass', 'instanceProp');
        $prop2 = new ReflectionProperty('TestClass', 'staticProp');
        
        return [
            $prop1->isStatic(),
            $prop2->isStatic()
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
fn test_reflection_property_is_default() {
    let result = run_php(
        r#"<?php
        class TestClass {
            public $declaredProp;
        }
        
        $prop = new ReflectionProperty('TestClass', 'declaredProp');
        return $prop->isDefault();
        "#,
    );
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_property_get_modifiers() {
    let result = run_php(
        r#"<?php
        class TestClass {
            public $publicProp;
            private $privateProp;
            protected $protectedProp;
            public static $staticProp;
        }
        
        $prop1 = new ReflectionProperty('TestClass', 'publicProp');
        $prop2 = new ReflectionProperty('TestClass', 'privateProp');
        $prop3 = new ReflectionProperty('TestClass', 'protectedProp');
        $prop4 = new ReflectionProperty('TestClass', 'staticProp');
        
        return [
            $prop1->getModifiers(),  // 1 (IS_PUBLIC)
            $prop2->getModifiers(),  // 4 (IS_PRIVATE)
            $prop3->getModifiers(),  // 2 (IS_PROTECTED)
            $prop4->getModifiers()   // 17 (IS_PUBLIC + IS_STATIC)
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
fn test_reflection_property_get_declaring_class() {
    let result = run_php(
        r#"<?php
        class TestClass {
            public $prop;
        }
        
        $prop = new ReflectionProperty('TestClass', 'prop');
        $class = $prop->getDeclaringClass();
        return $class->getName();
        "#,
    );
    assert_eq!(result, Val::String(std::rc::Rc::new(b"TestClass".to_vec())));
}

#[test]
fn test_reflection_property_to_string() {
    let result = run_php(
        r#"<?php
        class TestClass {
            public $publicProp;
        }
        
        $prop = new ReflectionProperty('TestClass', 'publicProp');
        return $prop->__toString();
        "#,
    );
    if let Val::String(s) = result {
        let output = String::from_utf8_lossy(&s);
        assert!(output.contains("public"));
        assert!(output.contains("TestClass"));
        assert!(output.contains("publicProp"));
    } else {
        panic!("Expected string result");
    }
}

#[test]
fn test_reflection_property_get_value() {
    let result = run_php(
        r#"<?php
        class TestClass {
            public $prop = 'test value';
        }
        
        $obj = new TestClass();
        $prop = new ReflectionProperty('TestClass', 'prop');
        return $prop->getValue($obj);
        "#,
    );
    assert_eq!(result, Val::String(std::rc::Rc::new(b"test value".to_vec())));
}

#[test]
fn test_reflection_property_set_value() {
    let result = run_php(
        r#"<?php
        class TestClass {
            public $prop = 'initial';
        }
        
        $obj = new TestClass();
        $prop = new ReflectionProperty('TestClass', 'prop');
        $prop->setValue($obj, 'modified');
        return $obj->prop;
        "#,
    );
    assert_eq!(result, Val::String(std::rc::Rc::new(b"modified".to_vec())));
}

#[test]
fn test_reflection_property_with_inheritance() {
    let result = run_php(
        r#"<?php
        class ParentClass {
            public $parentProp;
        }
        
        class ChildClass extends ParentClass {
            public $childProp;
        }
        
        $prop1 = new ReflectionProperty('ChildClass', 'childProp');
        $prop2 = new ReflectionProperty('ChildClass', 'parentProp');
        
        return [
            $prop1->getName(),
            $prop2->getName()
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
fn test_reflection_property_multiple_modifiers() {
    let result = run_php(
        r#"<?php
        class TestClass {
            private static $prop;
        }
        
        $prop = new ReflectionProperty('TestClass', 'prop');
        return [
            $prop->isPrivate(),
            $prop->isStatic(),
            $prop->getModifiers()  // Should be 4 + 16 = 20
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
fn test_reflection_property_get_attributes() {
    let result = run_php(r#"<?php
        class MyClass {
            public $prop;
        }
        
        $rp = new ReflectionProperty('MyClass', 'prop');
        return $rp->getAttributes();
    "#);
    
    // Should return empty array
    if let Val::Array(arr) = result {
        assert_eq!(arr.map.len(), 0);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_reflection_property_has_type() {
    let result = run_php(r#"<?php
        class MyClass {
            public $prop;
        }
        
        $rp = new ReflectionProperty('MyClass', 'prop');
        return $rp->hasType();
    "#);
    
    assert_eq!(result, Val::Bool(false));
}

#[test]
fn test_reflection_property_is_initialized() {
    let result = run_php(r#"<?php
        class MyClass {
            public $prop = 42;
        }
        
        $obj = new MyClass();
        $rp = new ReflectionProperty('MyClass', 'prop');
        return $rp->isInitialized($obj);
    "#);
    
    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_property_set_accessible() {
    let result = run_php(r#"<?php
        class MyClass {
            private $prop = 42;
        }
        
        $obj = new MyClass();
        $rp = new ReflectionProperty('MyClass', 'prop');
        $rp->setAccessible(true);
        return $rp->getValue($obj);
    "#);
    
    assert_eq!(result, Val::Int(42));
}

#[test]
fn test_reflection_property_get_raw_default_value() {
    let result = run_php(r#"<?php
        class MyClass {
            public static $staticProp = 123;
        }
        
        $rp = new ReflectionProperty('MyClass', 'staticProp');
        return $rp->getRawDefaultValue();
    "#);
    
    assert_eq!(result, Val::Int(123));
}

#[test]
fn test_reflection_property_has_hooks() {
    let result = run_php(r#"<?php
        class MyClass {
            public $prop;
        }
        
        $rp = new ReflectionProperty('MyClass', 'prop');
        return $rp->hasHooks();
    "#);
    
    assert_eq!(result, Val::Bool(false));
}

#[test]
fn test_reflection_property_get_hooks() {
    let result = run_php(r#"<?php
        class MyClass {
            public $prop;
        }
        
        $rp = new ReflectionProperty('MyClass', 'prop');
        $hooks = $rp->getHooks();
        return count($hooks);
    "#);
    
    assert_eq!(result, Val::Int(0));
}

#[test]
fn test_reflection_property_get_settable_type() {
    let result = run_php(r#"<?php
        class MyClass {
            public $prop;
        }
        
        $rp = new ReflectionProperty('MyClass', 'prop');
        return $rp->getSettableType();
    "#);
    
    assert_eq!(result, Val::Null);
}

#[test]
fn test_reflection_property_is_final() {
    let result = run_php(r#"<?php
        class MyClass {
            public $prop;
        }
        
        $rp = new ReflectionProperty('MyClass', 'prop');
        return $rp->isFinal();
    "#);
    
    assert_eq!(result, Val::Bool(false));
}

#[test]
fn test_reflection_property_is_lazy() {
    let result = run_php(r#"<?php
        class MyClass {
            public $prop;
        }
        
        $rp = new ReflectionProperty('MyClass', 'prop');
        return $rp->isLazy();
    "#);
    
    assert_eq!(result, Val::Bool(false));
}

#[test]
fn test_reflection_property_is_virtual() {
    let result = run_php(r#"<?php
        class MyClass {
            public $prop;
        }
        
        $rp = new ReflectionProperty('MyClass', 'prop');
        return $rp->isVirtual();
    "#);
    
    assert_eq!(result, Val::Bool(false));
}

