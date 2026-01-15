mod common;

use common::run_code_capture_output;

#[test]
fn test_parse_url_coerces_bool() {
    let (_, output) = run_code_capture_output(
        r#"<?php
        $parts = parse_url(false);
        echo $parts['path'];
        "#,
    )
    .expect("execution failed");

    assert_eq!(output, "");
}
