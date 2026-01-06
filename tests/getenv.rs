mod common;

use common::run_code_capture_output;

#[test]
fn test_getenv_string_and_array() {
    unsafe {
        std::env::set_var("PHP_RS_GETENV_TEST", "bar");
    }

    let code = r#"<?php
$value = getenv("PHP_RS_GETENV_TEST");
echo ($value === false ? "false" : $value) . "\n";
$env = getenv();
echo $env["PHP_RS_GETENV_TEST"] . "\n";
?>"#;

    let (_, output) = run_code_capture_output(code).unwrap();
    assert_eq!(output, "bar\nbar\n");
}

#[test]
fn test_getenv_missing_returns_false() {
    unsafe {
        std::env::remove_var("PHP_RS_GETENV_MISSING");
    }

    let code = r#"<?php
$value = getenv("PHP_RS_GETENV_MISSING");
echo ($value === false ? "false" : $value) . "\n";
?>"#;

    let (_, output) = run_code_capture_output(code).unwrap();
    assert_eq!(output, "false\n");
}

#[test]
fn test_getenv_local_only() {
    unsafe {
        std::env::set_var("PHP_RS_GETENV_LOCAL_ONLY", "baz");
    }

    let code = r#"<?php
$value = getenv("PHP_RS_GETENV_LOCAL_ONLY", true);
echo ($value === false ? "false" : $value) . "\n";
?>"#;

    let (_, output) = run_code_capture_output(code).unwrap();
    assert_eq!(output, "baz\n");
}
