mod common;

use common::run_code_capture_output;

#[test]
fn test_set_error_handler_invokes_callback() {
    let (_, output) = run_code_capture_output(
        r#"<?php
        set_error_handler(function($errno, $errstr) { echo 'handled'; return true; });
        trigger_error('boom', E_USER_WARNING);
        "#,
    )
    .expect("execution failed");

    assert!(output.contains("handled"));
}
