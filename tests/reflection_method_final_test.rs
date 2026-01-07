mod common;
use common::run_code_capture_output;

#[test]
fn reflection_method_is_final() {
    let script = r#"<?php
        class Foo {
            final public function bar() {}
            public function baz() {}
        }
        
        $rc = new ReflectionClass('Foo');
        var_dump($rc->getMethod('bar')->isFinal());
        var_dump($rc->getMethod('baz')->isFinal());
        
        // Internal classes
        $rc = new ReflectionClass('ReflectionClass');
        var_dump($rc->getMethod('getName')->isFinal());
        
        // Modifiers test (IS_FINAL = 4)
        var_dump(($rc->getMethod('getName')->getModifiers() & 4) === 0);
        $rc2 = new ReflectionClass('Foo');
        var_dump(($rc2->getMethod('bar')->getModifiers() & 4) === 4);
    "#;
    let (_val, output) = run_code_capture_output(script).expect("Execution failed");
    assert!(output.contains("bool(true)"));
    assert!(output.contains("bool(false)"));
    assert!(output.contains("bool(false)"));
}
