mod common;
use common::run_code_capture_output;

#[test]
fn test_foreach_null_is_warning() {
    let code = r#"<?php
        $items = null;
        foreach ($items as $item) {
            echo $item;
        }
        echo "ok";
    "#;
    let (_val, output) = run_code_capture_output(code).expect("Execution failed");
    assert!(output.contains("ok"));
}
