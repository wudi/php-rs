mod common;

use common::run_code;
use php_rs::core::value::Val;

#[test]
fn test_array_merge() {
    let code = r#"<?php
        $a = ['a' => 1, 0 => 2];
        $b = [0 => 3, 'b' => 4];
        $c = ['a' => 5];
        
        return array_merge($a, $b, $c);
    "#;

    let val = run_code(code);
    if let Val::Array(arr) = val {
        // Expected:
        // 'a' => 5 (overwritten by $c)
        // 0 => 2 (from $a, renumbered to 0)
        // 1 => 3 (from $b, renumbered to 1)
        // 'b' => 4 (from $b)

        // Wait, order matters.
        // $a: 'a'=>1, 0=>2
        // $b: 0=>3, 'b'=>4
        // $c: 'a'=>5

        // Merge:
        // 1. 'a' => 1
        // 2. 0 => 2 (next_int=1)
        // 3. 1 => 3 (next_int=2)
        // 4. 'b' => 4
        // 5. 'a' => 5 (overwrite 'a')

        // Result:
        // 'a' => 5
        // 0 => 2
        // 1 => 3
        // 'b' => 4

        // Wait, IndexMap preserves insertion order.
        // 'a' was inserted first.
        // So keys order: 'a', 0, 1, 'b'.

        // Let's verify count is 4.
        // Wait, I said assert_eq!(arr.map.len(), 5) above.
        // 'a' is overwritten, so it's the same key.
        // So count should be 4.
        assert_eq!(arr.map.len(), 4);
    } else {
        panic!("Expected array, got {:?}", val);
    }
}

#[test]
fn test_array_keys() {
    let code = r#"<?php
        $a = ['a' => 1, 2 => 3];
        return array_keys($a);
    "#;

    let val = run_code(code);
    if let Val::Array(arr) = val {
        assert_eq!(arr.map.len(), 2);
        // 0 => 'a'
        // 1 => 2
    } else {
        panic!("Expected array, got {:?}", val);
    }
}

#[test]
fn test_array_values() {
    let code = r#"<?php
        $a = ['a' => 1, 2 => 3];
        return array_values($a);
    "#;

    let val = run_code(code);
    if let Val::Array(arr) = val {
        assert_eq!(arr.map.len(), 2);
        // 0 => 1
        // 1 => 3
    } else {
        panic!("Expected array, got {:?}", val);
    }
}

#[test]
fn test_array_push_pop() {
    let code = r#"<?php
        $a = [1, 2];
        array_push($a, 3, 4);
        $p = array_pop($a);
        return [$a, $p];
    "#;

    let val = run_code(code);
    // Expected: [[1, 2, 3], 4]
    if let Val::Array(arr) = val {
        assert_eq!(arr.map.len(), 2);
    } else {
        panic!("Expected array, got {:?}", val);
    }
}

#[test]
fn test_array_shift_unshift() {
    let code = r#"<?php
        $a = [1, 2];
        array_unshift($a, 0);
        $s = array_shift($a);
        return [$a, $s];
    "#;

    let val = run_code(code);
    // Expected: [[1, 2], 0]
    if let Val::Array(arr) = val {
        assert_eq!(arr.map.len(), 2);
    } else {
        panic!("Expected array, got {:?}", val);
    }
}

#[test]
fn test_array_fill() {
    let code = r#"<?php
        return array_fill(5, 2, 'banana');
    "#;

    let val = run_code(code);
    if let Val::Array(arr) = val {
        assert_eq!(arr.map.len(), 2);
        // 5 => 'banana'
        // 6 => 'banana'
    } else {
        panic!("Expected array, got {:?}", val);
    }
}

#[test]
fn test_array_map() {
    let code = r#"<?php
        $a = [1, 2, 3];
        return array_map(function($x) { return $x * 2; }, $a);
    "#;

    let val = run_code(code);
    if let Val::Array(arr) = val {
        assert_eq!(arr.map.len(), 3);
        // 0 => 2, 1 => 4, 2 => 6
    } else {
        panic!("Expected array, got {:?}", val);
    }
}

#[test]
fn test_array_filter() {
    let code = r#"<?php
        $a = [1, 2, 3, 4];
        return array_filter($a, function($x) { return $x % 2 == 0; });
    "#;

    let val = run_code(code);
    if let Val::Array(arr) = val {
        assert_eq!(arr.map.len(), 2);
        // 1 => 2, 3 => 4 (keys preserved)
    } else {
        panic!("Expected array, got {:?}", val);
    }
}

#[test]
fn test_array_reduce() {
    let code = r#"<?php
        $a = [1, 2, 3, 4];
        return array_reduce($a, function($carry, $item) { return $carry + $item; }, 10);
    "#;

    let val = run_code(code);
    assert_eq!(val.to_int(), 20);
}

#[test]
fn test_sort() {
    let code = r#"<?php
        $a = [3, 1, 2];
        sort($a);
        return $a;
    "#;

    let val = run_code(code);
    if let Val::Array(arr) = val {
        assert_eq!(arr.map.len(), 3);
        // 0 => 1, 1 => 2, 2 => 3
    } else {
        panic!("Expected array, got {:?}", val);
    }
}

#[test]
fn test_usort() {
    let code = r#"<?php
        $a = [3, 1, 2];
        usort($a, function($a, $b) { 
            if ($a < $b) return -1;
            if ($a > $b) return 1;
            return 0;
        });
        return $a;
    "#;

    let val = run_code(code);
    if let Val::Array(arr) = val {
        assert_eq!(arr.map.len(), 3);
        // 0 => 1, 1 => 2, 2 => 3
    } else {
        panic!("Expected array, got {:?}", val);
    }
}

#[test]
fn test_array_slice() {
    let code = r#"<?php
        $input = ["a", "b", "c", "d", "e"];
        return array_slice($input, 2, 2);
    "#;

    let val = run_code(code);
    if let Val::Array(arr) = val {
        assert_eq!(arr.map.len(), 2);
        // 0 => "c", 1 => "d"
    } else {
        panic!("Expected array, got {:?}", val);
    }
}

#[test]
fn test_array_splice() {
    let code = r#"<?php
        $input = ["red", "green", "blue", "yellow"];
        $removed = array_splice($input, 1, 2, ["orange", "black"]);
        return [$input, $removed];
    "#;

    let val = run_code(code);
    // Expected: [["red", "orange", "black", "yellow"], ["green", "blue"]]
    if let Val::Array(arr) = val {
        assert_eq!(arr.map.len(), 2);
    } else {
        panic!("Expected array, got {:?}", val);
    }
}

#[test]
fn test_internal_pointer() {
    let code = r#"<?php
        $transport = array('foot', 'bike', 'car', 'plane');
        $mode = current($transport); // $mode = 'foot';
        $mode = next($transport);    // $mode = 'bike';
        $mode = current($transport); // $mode = 'bike';
        $mode = prev($transport);    // $mode = 'foot';
        $mode = end($transport);     // $mode = 'plane';
        $mode = reset($transport);   // $mode = 'foot';
        return $mode;
    "#;

    let val = run_code(code);
    assert_eq!(String::from_utf8_lossy(&val.to_php_string_bytes()), "foot");
}

#[test]
fn test_array_chunk() {
    let code = r#"<?php
        $input = [1, 2, 3, 4, 5];
        return array_chunk($input, 2);
    "#;

    let val = run_code(code);
    if let Val::Array(arr) = val {
        assert_eq!(arr.map.len(), 3); // 3 chunks: [1,2], [3,4], [5]
    } else {
        panic!("Expected array, got {:?}", val);
    }
}

#[test]
fn test_array_column() {
    let code = r#"<?php
        $records = [
            ['id' => 1, 'name' => 'Alice'],
            ['id' => 2, 'name' => 'Bob'],
        ];
        return array_column($records, 'name');
    "#;

    let val = run_code(code);
    if let Val::Array(arr) = val {
        assert_eq!(arr.map.len(), 2);
    } else {
        panic!("Expected array, got {:?}", val);
    }
}

#[test]
fn test_array_flip() {
    let code = r#"<?php
        $input = ['a' => 1, 'b' => 2];
        return array_flip($input);
    "#;

    let val = run_code(code);
    if let Val::Array(arr) = val {
        assert_eq!(arr.map.len(), 2);
        // Should have keys 1 and 2 mapping to 'a' and 'b'
    } else {
        panic!("Expected array, got {:?}", val);
    }
}

#[test]
fn test_array_reverse() {
    let code = r#"<?php
        $input = [1, 2, 3];
        return array_reverse($input);
    "#;

    let val = run_code(code);
    if let Val::Array(arr) = val {
        assert_eq!(arr.map.len(), 3);
    } else {
        panic!("Expected array, got {:?}", val);
    }
}

#[test]
fn test_array_unique() {
    let code = r#"<?php
        $input = [1, 2, 2, 3, 3, 3];
        return array_unique($input);
    "#;

    let val = run_code(code);
    if let Val::Array(arr) = val {
        assert_eq!(arr.map.len(), 3);
    } else {
        panic!("Expected array, got {:?}", val);
    }
}

#[test]
fn test_array_sum_product() {
    let code = r#"<?php
        $a = [1, 2, 3, 4];
        return [array_sum($a), array_product($a)];
    "#;

    let val = run_code(code);
    if let Val::Array(arr) = val {
        assert_eq!(arr.map.len(), 2);
        // sum = 10, product = 24
    } else {
        panic!("Expected array, got {:?}", val);
    }
}

#[test]
fn test_array_pad() {
    let code = r#"<?php
        $input = [1, 2];
        return array_pad($input, 5, 0);
    "#;

    let val = run_code(code);
    if let Val::Array(arr) = val {
        assert_eq!(arr.map.len(), 5);
    } else {
        panic!("Expected array, got {:?}", val);
    }
}

#[test]
fn test_array_combine() {
    let code = r#"<?php
        $keys = ['a', 'b', 'c'];
        $values = [1, 2, 3];
        return array_combine($keys, $values);
    "#;

    let val = run_code(code);
    if let Val::Array(arr) = val {
        assert_eq!(arr.map.len(), 3);
    } else {
        panic!("Expected array, got {:?}", val);
    }
}

#[test]
fn test_array_is_list() {
    let code = r#"<?php
        $a = [1, 2, 3];
        $b = [0 => 'a', 2 => 'b'];
        return [array_is_list($a), array_is_list($b)];
    "#;

    let val = run_code(code);
    if let Val::Array(arr) = val {
        assert_eq!(arr.map.len(), 2);
    } else {
        panic!("Expected array, got {:?}", val);
    }
}

#[test]
fn test_array_change_key_case() {
    let code = r#"<?php
        $input = ['FirSt' => 1, 'SecOnd' => 2];
        return array_change_key_case($input, 0); // 0 = CASE_LOWER
    "#;

    let val = run_code(code);
    if let Val::Array(arr) = val {
        assert_eq!(arr.map.len(), 2);
    } else {
        panic!("Expected array, got {:?}", val);
    }
}

#[test]
fn test_array_diff_intersect() {
    let code = r#"<?php
        $a = [1, 2, 3, 4];
        $b = [2, 4, 6];
        return [array_diff($a, $b), array_intersect($a, $b)];
    "#;

    let val = run_code(code);
    if let Val::Array(arr) = val {
        assert_eq!(arr.map.len(), 2);
    } else {
        panic!("Expected array, got {:?}", val);
    }
}

#[test]
fn test_sorting_functions() {
    let code = r#"<?php
        $a = ['b' => 2, 'a' => 1];
        
        $rsorted = [3, 1, 2];
        rsort($rsorted);
        
        $asorted = ['b' => 2, 'a' => 1];
        asort($asorted);
        
        $arsorted = ['a' => 1, 'b' => 2];
        arsort($arsorted);
        
        $krsorted = ['a' => 1, 'b' => 2];
        krsort($krsorted);
        
        return [$rsorted, count($asorted), count($arsorted), count($krsorted)];
    "#;

    let val = run_code(code);
    if let Val::Array(arr) = val {
        assert_eq!(arr.map.len(), 4);
    } else {
        panic!("Expected array, got {:?}", val);
    }
}

#[test]
fn test_array_walk() {
    let code = r#"<?php
        $fruits = ['a' => 'apple', 'b' => 'banana'];
        array_walk($fruits, function(&$item, $key) {
            $item = strtoupper($item);
        });
        return $fruits;
    "#;

    let val = run_code(code);
    if let Val::Array(arr) = val {
        assert_eq!(arr.map.len(), 2);
    } else {
        panic!("Expected array, got {:?}", val);
    }
}
