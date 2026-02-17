use php_rs::vm::executor::{ExecutionConfig, execute_code_with_config};

#[test]
fn test_error_reporting_zero_suppresses_warnings() {
    let code = r#"<?php
error_reporting(0);
foreach (null as $x) {}
return 1;
"#;

    let mut config = ExecutionConfig::default();
    config.capture_output = true;
    let result = execute_code_with_config(code, config).expect("execution failed");
    assert!(result.stderr.trim().is_empty());
}
