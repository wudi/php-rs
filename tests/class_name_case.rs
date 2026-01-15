mod common;
use common::run_code_capture_output;

#[test]
fn test_class_name_case_insensitive() {
    let code = r#"<?php
        class CaseExample {}
        var_dump(class_exists('caseexample'));
        $obj = new caseexample();
        var_dump($obj instanceof CaseExample);
    "#;

    let (_val, output) = run_code_capture_output(code).expect("Execution failed");
    assert!(output.contains("bool(true)"));
}
