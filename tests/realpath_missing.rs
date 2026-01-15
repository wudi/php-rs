mod common;

use common::run_code_capture_output;

#[test]
fn test_realpath_missing_returns_false() {
    let (_, output) = run_code_capture_output(
        r#"<?php
        var_dump(realpath('/no/such/file'));
        "#,
    )
    .expect("execution failed");

    assert_eq!(output.trim(), "bool(false)");
}
