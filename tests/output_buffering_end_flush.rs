use php_rs::vm::executor::execute_code;

#[test]
fn test_output_buffer_flushed_on_shutdown() {
    let result = execute_code("<?php ob_start(); echo 'hi';").unwrap();
    assert_eq!(result.stdout, "hi");
}
