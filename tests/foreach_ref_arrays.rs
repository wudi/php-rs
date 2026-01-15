mod common;

use common::run_code;
use php_rs::core::value::Val;

#[test]
fn test_foreach_by_ref_updates_array_elements() {
    let code = r#"<?php
$iterations = array(array(10, 20), array(30));
$priorities = array(5, 10, 20);

foreach ($iterations as $index => &$iteration) {
    $current = current($iteration);
    if (false === $current) {
        continue;
    }
    $iteration = $priorities;
}

return $iterations[0][0];
"#;

    let val = run_code(code);
    assert_eq!(val, Val::Int(5));
}
