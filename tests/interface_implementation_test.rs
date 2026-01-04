mod common;

use common::run_php;
use php_rs::core::value::Val;

#[test]
fn test_countable_interface_basic() {
    let code = r#"
    <?php
    class MyCounter implements Countable {
        private $count = 7;
        
        public function count(): int {
            return $this->count;
        }
    }
    
    $counter = new MyCounter();
    return count($counter);
    "#;
    
    let result = run_php(code);
    assert_eq!(result, Val::Int(7), "count() should return 7 from Countable::count()");
}

#[test]
fn test_countable_interface_with_logic() {
    let code = r#"
    <?php
    class ItemCollection implements Countable {
        private $items = ['a', 'b', 'c', 'd'];
        
        public function count(): int {
            return count($this->items);
        }
    }
    
    $collection = new ItemCollection();
    return count($collection);
    "#;
    
    let result = run_php(code);
    assert_eq!(result, Val::Int(4), "count() should return 4");
}

#[test]
fn test_countable_multiple_instances() {
    let code = r#"
    <?php
    class Counter implements Countable {
        private $value;
        
        public function __construct($val) {
            $this->value = $val;
        }
        
        public function count(): int {
            return $this->value;
        }
    }
    
    $c1 = new Counter(5);
    $c2 = new Counter(10);
    $c3 = new Counter(15);
    
    return count($c1) + count($c2) + count($c3);
    "#;
    
    let result = run_php(code);
    assert_eq!(result, Val::Int(30), "sum of counts should be 30");
}

#[test]
fn test_iterator_interface_basic() {
    let code = r#"
    <?php
    class SimpleIterator implements Iterator {
        private $items = [10, 20, 30];
        private $position = 0;
        
        public function current(): mixed {
            return $this->items[$this->position];
        }
        
        public function key(): mixed {
            return $this->position;
        }
        
        public function next(): void {
            $this->position++;
        }
        
        public function rewind(): void {
            $this->position = 0;
        }
        
        public function valid(): bool {
            return isset($this->items[$this->position]);
        }
    }
    
    $sum = 0;
    foreach (new SimpleIterator() as $value) {
        $sum += $value;
    }
    return $sum;
    "#;
    
    let result = run_php(code);
    assert_eq!(result, Val::Int(60), "sum should be 60");
}

#[test]
fn test_iterator_with_keys() {
    let code = r#"
    <?php
    class KeyValueIterator implements Iterator {
        private $data = ['a' => 1, 'b' => 2, 'c' => 3];
        private $keys;
        private $position = 0;
        
        public function __construct() {
            $this->keys = array_keys($this->data);
        }
        
        public function current(): mixed {
            $key = $this->keys[$this->position];
            return $this->data[$key];
        }
        
        public function key(): mixed {
            return $this->keys[$this->position];
        }
        
        public function next(): void {
            $this->position++;
        }
        
        public function rewind(): void {
            $this->position = 0;
        }
        
        public function valid(): bool {
            return $this->position < count($this->keys);
        }
    }
    
    $result = '';
    foreach (new KeyValueIterator() as $key => $value) {
        $result .= $key . $value;
    }
    return $result;
    "#;
    
    let result = run_php(code);
    if let Val::String(s) = result {
        assert_eq!(&s[..], b"a1b2c3", "should concatenate keys and values");
    } else {
        panic!("Expected string result");
    }
}

#[test]
fn test_iterator_empty_collection() {
    let code = r#"
    <?php
    class EmptyIterator implements Iterator {
        public function current(): mixed { return null; }
        public function key(): mixed { return null; }
        public function next(): void {}
        public function rewind(): void {}
        public function valid(): bool { return false; }
    }
    
    $count = 0;
    foreach (new EmptyIterator() as $item) {
        $count++;
    }
    return $count;
    "#;
    
    let result = run_php(code);
    assert_eq!(result, Val::Int(0), "empty iterator should not iterate");
}

#[test]
fn test_iterator_break_in_loop() {
    let code = r#"
    <?php
    class NumberIterator implements Iterator {
        private $max = 100;
        private $current = 0;
        
        public function current(): mixed { return $this->current; }
        public function key(): mixed { return $this->current; }
        public function next(): void { $this->current++; }
        public function rewind(): void { $this->current = 0; }
        public function valid(): bool { return $this->current < $this->max; }
    }
    
    $sum = 0;
    foreach (new NumberIterator() as $num) {
        $sum += $num;
        if ($num >= 4) break;
    }
    return $sum;
    "#;
    
    let result = run_php(code);
    assert_eq!(result, Val::Int(10), "sum should be 0+1+2+3+4 = 10");
}

#[test]
fn test_nested_iterator_loops() {
    let code = r#"
    <?php
    class TwoItemIterator implements Iterator {
        private $items;
        private $position = 0;
        
        public function __construct($a, $b) {
            $this->items = [$a, $b];
        }
        
        public function current(): mixed { return $this->items[$this->position]; }
        public function key(): mixed { return $this->position; }
        public function next(): void { $this->position++; }
        public function rewind(): void { $this->position = 0; }
        public function valid(): bool { return $this->position < count($this->items); }
    }
    
    $result = 0;
    foreach (new TwoItemIterator(1, 2) as $outer) {
        foreach (new TwoItemIterator(10, 20) as $inner) {
            $result += $outer * $inner;
        }
    }
    return $result;
    "#;
    
    let result = run_php(code);
    // (1*10 + 1*20) + (2*10 + 2*20) = 30 + 60 = 90
    assert_eq!(result, Val::Int(90), "nested loops should work correctly");
}

#[test]
fn test_countable_and_iterator_together() {
    let code = r#"
    <?php
    class CountableIterator implements Countable, Iterator {
        private $items = [5, 10, 15];
        private $position = 0;
        
        public function count(): int {
            return count($this->items);
        }
        
        public function current(): mixed { return $this->items[$this->position]; }
        public function key(): mixed { return $this->position; }
        public function next(): void { $this->position++; }
        public function rewind(): void { $this->position = 0; }
        public function valid(): bool { return $this->position < count($this->items); }
    }
    
    $obj = new CountableIterator();
    $itemCount = count($obj);
    
    $sum = 0;
    foreach ($obj as $value) {
        $sum += $value;
    }
    
    return $itemCount * 1000 + $sum;
    "#;
    
    let result = run_php(code);
    // count = 3, sum = 30, result = 3*1000 + 30 = 3030
    assert_eq!(result, Val::Int(3030), "both interfaces should work on same object");
}

#[test]
fn test_iterator_method_call_order() {
    let code = r#"
    <?php
    class TrackedIterator implements Iterator {
        private $items = ['x'];
        private $position = 0;
        public $calls = [];
        
        public function current(): mixed {
            $this->calls[] = 'current';
            return $this->items[$this->position];
        }
        
        public function key(): mixed {
            $this->calls[] = 'key';
            return $this->position;
        }
        
        public function next(): void {
            $this->calls[] = 'next';
            $this->position++;
        }
        
        public function rewind(): void {
            $this->calls[] = 'rewind';
            $this->position = 0;
        }
        
        public function valid(): bool {
            $this->calls[] = 'valid';
            return $this->position < count($this->items);
        }
    }
    
    $iter = new TrackedIterator();
    foreach ($iter as $k => $v) {
        // Just iterate once
    }
    
    // rewind should be called first
    return $iter->calls[0];
    "#;
    
    let result = run_php(code);
    if let Val::String(s) = result {
        assert_eq!(&s[..], b"rewind", "rewind should be first method called");
    } else {
        panic!("Expected string result");
    }
}

#[test]
fn test_non_countable_object_returns_one() {
    let code = r#"
    <?php
    class RegularClass {
        public $prop = 'value';
    }
    
    $obj = new RegularClass();
    return count($obj);
    "#;
    
    let result = run_php(code);
    assert_eq!(result, Val::Int(1), "count() on non-Countable object should return 1");
}

#[test]
fn test_arrayaccess_still_works() {
    let code = r#"
    <?php
    class MyArray implements ArrayAccess {
        private $data = [];
        
        public function offsetExists($offset): bool {
            return isset($this->data[$offset]);
        }
        
        public function offsetGet($offset): mixed {
            return $this->data[$offset] ?? null;
        }
        
        public function offsetSet($offset, $value): void {
            $this->data[$offset] = $value;
        }
        
        public function offsetUnset($offset): void {
            unset($this->data[$offset]);
        }
    }
    
    $arr = new MyArray();
    $arr['test'] = 42;
    return $arr['test'];
    "#;
    
    let result = run_php(code);
    assert_eq!(result, Val::Int(42), "ArrayAccess should still work");
}

#[test]
fn test_countable_inheritance() {
    let code = r#"
    <?php
    class BaseCounter implements Countable {
        protected $value = 5;
        
        public function count(): int {
            return $this->value;
        }
    }
    
    class ChildCounter extends BaseCounter {
        public function __construct() {
            $this->value = 10;
        }
    }
    
    $child = new ChildCounter();
    return count($child);
    "#;
    
    let result = run_php(code);
    assert_eq!(result, Val::Int(10), "inherited Countable should work");
}

#[test]
fn test_iterator_with_modification() {
    let code = r#"
    <?php
    class ModifiableIterator implements Iterator {
        public $items = [1, 2, 3];
        private $position = 0;
        
        public function current(): mixed { return $this->items[$this->position]; }
        public function key(): mixed { return $this->position; }
        public function next(): void { $this->position++; }
        public function rewind(): void { $this->position = 0; }
        public function valid(): bool { return $this->position < count($this->items); }
    }
    
    $iter = new ModifiableIterator();
    $sum = 0;
    foreach ($iter as $value) {
        $sum += $value;
    }
    
    // Can be reused
    $sum2 = 0;
    foreach ($iter as $value) {
        $sum2 += $value;
    }
    
    return $sum + $sum2;
    "#;
    
    let result = run_php(code);
    assert_eq!(result, Val::Int(12), "iterator should be reusable");
}
