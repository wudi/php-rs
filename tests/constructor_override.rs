mod common;

use common::run_code_capture_output;

#[test]
fn test_constructor_override_allows_signature_change() {
    let (_, output) = run_code_capture_output(
        r#"<?php
        class Base {
            public function __construct($a = null, $b = 1, $c = []) {}
        }
        class Child extends Base {
            public function __construct($x) {}
        }
        echo "ok";
        "#,
    )
    .expect("execution failed");

    assert_eq!(output, "ok");
}
