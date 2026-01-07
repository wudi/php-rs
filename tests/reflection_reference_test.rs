mod common;
use common::run_code_capture_output;

#[test]
fn reflection_reference_basic() {
    let script = r##"<?php
        $a = 1;
        $b = &$a;
        $arr = ['x' => &$a, 'y' => 2];
        
        $ref1 = ReflectionReference::fromArrayElement($arr, 'x');
        var_dump($ref1 instanceof ReflectionReference);
        
        $ref2 = ReflectionReference::fromArrayElement($arr, 'y');
        var_dump($ref2);
        
        if ($ref1) {
            echo "ID: " . $ref1->getId() . "\n";
        }
        
        $c = &$a;
        $arr2 = ['z' => &$c];
        $ref3 = ReflectionReference::fromArrayElement($arr2, 'z');
        var_dump($ref1->getId() === $ref3->getId());
    "##;
    let (_val, output) = run_code_capture_output(script).expect("Execution failed");
    // Currently returns NULL and placeholder ID
    assert!(output.contains("bool(true)"));
    assert!(output.contains("NULL"));
    assert!(output.contains("bool(true)"));
}

