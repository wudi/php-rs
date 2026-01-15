mod common;
use common::run_code_capture_output;

#[test]
fn test_mysqli_report_constants() {
    let code = r#"<?php
echo defined('MYSQLI_REPORT_OFF') ? '1' : '0';
echo '|', MYSQLI_REPORT_OFF;
echo '|', MYSQLI_REPORT_ERROR;
echo '|', MYSQLI_REPORT_STRICT;
echo '|', MYSQLI_REPORT_INDEX;
echo '|', MYSQLI_REPORT_CLOSE;
echo '|', MYSQLI_REPORT_ALL;
"#;
    let (_, output) = run_code_capture_output(code).expect("execution failed");
    assert_eq!(output, "1|0|1|2|4|8|255");
}

#[test]
fn test_mysqli_report_accepts_flags() {
    let code = r#"<?php
echo mysqli_report(MYSQLI_REPORT_OFF) ? '1' : '0';
"#;
    let (_, output) = run_code_capture_output(code).expect("execution failed");
    assert_eq!(output, "1");
}
