mod common;

use common::run_code_capture_output;

#[test]
fn test_strpos_coerces_bool() {
    let (_, output) = run_code_capture_output(
        r#"<?php
        echo strpos(false, "");
        "#,
    )
    .expect("execution failed");

    assert_eq!(output, "0");
}
