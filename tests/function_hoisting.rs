mod common;

use common::run_code_capture_output;

#[test]
fn test_function_called_before_definition() {
    let (_, output) = run_code_capture_output(
        r#"<?php
        echo wp_hoist_test();
        function wp_hoist_test() {
            return "ok";
        }
        "#,
    )
    .expect("execution failed");

    assert_eq!(output, "ok");
}
