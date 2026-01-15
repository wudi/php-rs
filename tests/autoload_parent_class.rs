mod common;

use common::run_code_capture_output;

#[test]
fn test_autoload_parent_class_on_definition() {
    let (_, output) = run_code_capture_output(
        r#"<?php
        spl_autoload_register(function($class) {
            if ($class === 'ParentC') {
                class ParentC {}
            }
        });
        class ChildC extends ParentC {}
        echo "ok";
        "#,
    )
    .expect("execution failed");

    assert_eq!(output, "ok");
}
