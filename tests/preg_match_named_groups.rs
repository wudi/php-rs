mod common;
use common::run_code_capture_output;

#[test]
fn test_preg_match_named_groups() {
    let code = r#"<?php
        $pattern = '/^(?P<scheme>[^:]+):\\/\\/(?P<host>[^\\/]+)/';
        $subject = 'https://example.com/path';
        preg_match($pattern, $subject, $matches);
        var_dump($matches['scheme']);
        var_dump($matches['host']);
    "#;

    let (_val, output) = run_code_capture_output(code).expect("Execution failed");
    assert!(output.contains("string(5) \"https\""));
    assert!(output.contains("string(11) \"example.com\""));
}
