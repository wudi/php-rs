mod common;

use common::run_code;
use php_rs::core::value::Val;

#[test]
fn test_foreach_over_class_const_array() {
    let code = r#"<?php
class Demo {
    public const ITEMS = [1, 2];
}

$sum = 0;
foreach (Demo::ITEMS as $val) {
    $sum += $val;
}
return $sum;
"#;

    let val = run_code(code);
    assert_eq!(val, Val::Int(3));
}
