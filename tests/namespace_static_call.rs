mod common;

use common::run_code_capture_output;

#[test]
fn test_namespaced_class_static_call() {
    let (_, output) = run_code_capture_output(
        r#"<?php
        namespace Foo\Bar;
        class Baz {
            public static function qux() {
                return "ok";
            }
        }
        namespace {
            echo Foo\Bar\Baz::qux();
        }
        "#,
    )
    .expect("execution failed");

    assert_eq!(output, "ok");
}
