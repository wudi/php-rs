mod common;

use common::run_code_capture_output;

#[test]
fn test_substr_coerces_bool() {
    let (_, output) = run_code_capture_output(
        r#"<?php
        echo substr(false, 0);
        "#,
    )
    .expect("execution failed");

    assert_eq!(output, "");
}
