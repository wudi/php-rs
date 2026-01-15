mod common;
use common::run_code_capture_output;

#[test]
fn test_array_callable_with_unpack() {
    let code = r#"<?php
        class Foo { public function bar($value) { var_dump($value); } }
        $cb = [new Foo(), 'bar'];
        $args = [42];
        $cb(...$args);
    "#;

    let (_val, output) = run_code_capture_output(code).expect("Execution failed");
    assert!(output.contains("int(42)"));
}
