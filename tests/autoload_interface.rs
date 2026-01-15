mod common;

use common::run_code_capture_output;

#[test]
fn test_autoload_interface_on_implements() {
    let (_, output) = run_code_capture_output(
        r#"<?php
        spl_autoload_register(function($class) {
            if ($class === 'IFace') {
                interface IFace {}
            }
        });
        class Impl implements IFace {}
        echo "ok";
        "#,
    )
    .expect("execution failed");

    assert_eq!(output, "ok");
}
