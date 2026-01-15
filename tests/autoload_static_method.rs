mod common;

use common::run_code_capture_output;

#[test]
fn test_autoload_for_static_method_call() {
    let (_, output) = run_code_capture_output(
        r#"<?php
        spl_autoload_register(function($class) {
            if ($class === 'Foo') {
                class Foo {
                    public static function bar() {
                        return "ok";
                    }
                }
            }
        });
        echo Foo::bar();
        "#,
    )
    .expect("execution failed");

    assert_eq!(output, "ok");
}
