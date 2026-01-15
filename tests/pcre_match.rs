mod common;
use common::run_code_capture_output;

#[test]
fn test_preg_match_lookbehind() {
    let code = r#"<?php
        $subject = "foobar";
        // Lookbehind: match 'bar' preceded by 'foo'
        preg_match('/(?<=foo)bar/', $subject, $matches);
        var_dump($matches);
    "#;
    let (_val, output) = run_code_capture_output(code).expect("Execution failed");
    assert!(output.contains(r#"string(3) "bar""#));
}

#[test]
fn test_preg_match_basic() {
    let code = r#"<?php
        $subject = "abcdef";
        preg_match('/^abc/', $subject, $matches);
        var_dump($matches);
    "#;
    let (_val, output) = run_code_capture_output(code).expect("Execution failed");
    assert!(output.contains(r#"string(3) "abc""#));
}

#[test]
fn test_preg_replace_basic() {
    let code = r#"<?php
        $subject = "quick brown fox";
        $result = preg_replace('/quick/', 'slow', $subject);
        var_dump($result);
    "#;
    let (_val, output) = run_code_capture_output(code).expect("Execution failed");
    assert!(output.contains(r#"string(14) "slow brown fox""#));
}

#[test]
fn test_preg_replace_backref() {
    let code = r#"<?php
        $subject = "April 15, 2003";
        $pattern = "/(\w+) (\d+), (\d+)/";
        $replacement = "$1 1, $3";
        $result = preg_replace($pattern, $replacement, $subject);
        var_dump($result);
    "#;
    let (_val, output) = run_code_capture_output(code).expect("Execution failed");
    assert!(output.contains(r#"string(13) "April 1, 2003""#));
}

#[test]
fn test_preg_match_all_basic() {
    let code = r#"<?php
        $subject = "foo bar baz";
        $count = preg_match_all('/[a-z]+/', $subject, $matches);
        var_dump($count);
        var_dump($matches);
    "#;
    let (_val, output) = run_code_capture_output(code).expect("Execution failed");
    assert!(output.contains("int(3)"));
    assert!(output.contains(r#"string(3) "foo""#));
    assert!(output.contains(r#"string(3) "bar""#));
    assert!(output.contains(r#"string(3) "baz""#));
}

#[test]
fn test_preg_replace_callback_basic() {
    let code = r#"<?php
        $subject = "foo 123 bar";
        $result = preg_replace_callback('/\d+/', function ($matches) {
            return '[' . $matches[0] . ']';
        }, $subject);
        var_dump($result);
    "#;
    let (_val, output) = run_code_capture_output(code).expect("Execution failed");
    assert!(output.contains(r#"string(13) "foo [123] bar""#));
}
