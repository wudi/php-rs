mod common;

use common::run_code_capture_output;

#[test]
fn test_strtoupper_coerces_non_string() {
    let (_, output) = run_code_capture_output(
        r#"<?php
        echo strtoupper(123) . "," . strtolower(true);
        "#,
    )
    .expect("execution failed");

    assert_eq!(output, "123,1");
}
