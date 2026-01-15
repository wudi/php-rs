mod common;

use common::run_code_capture_output;
use php_rs::core::value::Val;

#[test]
fn test_register_shutdown_function_executes() {
    let (value, output) = run_code_capture_output(
        "<?php register_shutdown_function(function() { echo 'done'; });",
    )
    .expect("execution failed");

    assert_eq!(value, Val::Null);
    assert!(output.contains("done"));
}
