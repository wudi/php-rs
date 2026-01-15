mod common;

use common::run_code_capture_output;

#[test]
fn test_json_serializable_interface_exists() {
    let (_, output) = run_code_capture_output(
        r#"<?php
        echo interface_exists('JsonSerializable') ? 'yes' : 'no';
        "#,
    )
    .expect("execution failed");

    assert_eq!(output, "yes");
}
