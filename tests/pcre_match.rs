mod common;

use common::run_code_capture_output;

#[test]
fn test_preg_match_coerces_subject_to_string() {
    let (_, output) = run_code_capture_output(
        r#"<?php
        $subject = 123;
        preg_match('/\d+/', $subject, $m);
        echo $m[0];
        "#,
    )
    .expect("execution failed");

    assert_eq!(output, "123");
}
