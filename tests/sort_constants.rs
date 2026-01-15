mod common;

use common::run_code_capture_output;

#[test]
fn test_sort_constants_defined() {
    let (_, output) = run_code_capture_output(
        r#"<?php
        echo SORT_REGULAR . "," . SORT_NUMERIC . "," . SORT_STRING . "," . SORT_LOCALE_STRING . "," . SORT_NATURAL . "," . SORT_FLAG_CASE . "," . SORT_ASC . "," . SORT_DESC;
        "#,
    )
    .expect("execution failed");

    assert_eq!(output, "0,1,2,5,6,8,4,3");
}
