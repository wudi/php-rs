mod common;
use common::run_code_capture_output;

#[test]
fn test_array_search_basic() {
    let code = r#"<?php
        $haystack = array('a', 'b', 'c');
        var_dump(array_search('b', $haystack));
        var_dump(array_search('d', $haystack));
        var_dump(array_search('1', array(1), true));
    "#;
    let (_val, output) = run_code_capture_output(code).expect("Execution failed");
    assert!(output.contains("int(1)"));
    assert!(output.contains("bool(false)"));
}
