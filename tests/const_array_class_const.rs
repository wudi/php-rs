mod common;
use common::run_code_capture_output;

#[test]
fn test_const_array_class_const_values() {
    let code = r#"<?php
        class Foo {}
        class Bar {
            const MAP = [Foo::class => Foo::class];
        }
        $map = Bar::MAP;
        var_dump($map[Foo::class]);
    "#;

    let (_val, output) = run_code_capture_output(code).expect("Execution failed");
    assert!(output.contains("string(3) \"Foo\""));
}
