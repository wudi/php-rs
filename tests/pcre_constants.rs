mod common;
use common::run_code_capture_output;

#[test]
fn test_pcre_constants_defined() {
    let code = r#"<?php
        var_dump(PREG_PATTERN_ORDER);
        var_dump(PREG_SET_ORDER);
        var_dump(PREG_OFFSET_CAPTURE);
        var_dump(PREG_UNMATCHED_AS_NULL);
        var_dump(PREG_SPLIT_NO_EMPTY);
        var_dump(PREG_SPLIT_DELIM_CAPTURE);
        var_dump(PREG_SPLIT_OFFSET_CAPTURE);
    "#;
    let (_val, output) = run_code_capture_output(code).expect("Execution failed");
    assert!(output.contains("int(1)"));
    assert!(output.contains("int(2)"));
    assert!(output.contains("int(256)"));
    assert!(output.contains("int(512)"));
    assert!(output.contains("int(1)"));
    assert!(output.contains("int(2)"));
    assert!(output.contains("int(4)"));
}
