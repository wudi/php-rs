mod common;
use common::run_code_capture_output;

#[test]
fn test_class_property_default_array() {
    let code = r#"<?php
        class Foo {
            public $bar = array('a', 'b');
        }
        $foo = new Foo();
        var_dump($foo->bar);
        var_dump(count($foo->bar));
    "#;
    let (_val, output) = run_code_capture_output(code).expect("Execution failed");
    assert!(output.contains("array(2)"));
    assert!(output.contains("int(2)"));
}
