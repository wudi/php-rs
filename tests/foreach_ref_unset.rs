mod common;

use common::run_code;
use php_rs::core::value::Val;

#[test]
fn test_unset_breaks_foreach_reference() {
    let code = r#"<?php
$arr = array(array(1), array(2));
foreach ($arr as &$v) {
    // no-op
}
unset($v);
$v = array(3);
return $arr[1][0];
"#;

    let val = run_code(code);
    assert_eq!(val, Val::Int(2));
}
