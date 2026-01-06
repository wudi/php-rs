# ReflectionClass::isSubclassOf Multi-level Inheritance Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make `ReflectionClass::isSubclassOf` match PHP semantics for multi-level inheritance and interfaces while returning false for the same class.

**Architecture:** Reuse the VM’s existing `is_subclass_of` helper for recursive parent/interface traversal, with an explicit same-class guard to preserve PHP behavior. Tests will assert class chains and interface inheritance through ReflectionClass.

**Tech Stack:** Rust, php-rs VM, reflection builtins, Rust integration tests.

### Task 1: Add failing ReflectionClass::isSubclassOf tests

**Files:**
- Modify: `tests/reflection_test.rs`
- Test: `tests/reflection_test.rs`

**Step 1: Write the failing test**

```rust
use php_rs::core::value::{ArrayKey, Val};
use php_rs::vm::engine::VM;

fn get_array_idx(vm: &VM, val: &Val, idx: i64) -> Val {
    if let Val::Array(arr) = val {
        let key = ArrayKey::Int(idx);
        let handle = arr.map.get(&key).expect("Array index not found");
        vm.arena.get(*handle).value.clone()
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_reflection_class_is_subclass_of_multilevel_and_interfaces() {
    let (result, vm) = common::run_code_with_vm(r#"<?php
        interface BaseInterface {}
        interface ChildInterface extends BaseInterface {}
        class GrandParentClass {}
        class ParentClass extends GrandParentClass {}
        class ChildClass extends ParentClass implements ChildInterface {}

        $rc_child = new ReflectionClass('ChildClass');
        $rc_parent = new ReflectionClass('ParentClass');
        $rc_interface = new ReflectionClass('ChildInterface');

        return [
            $rc_child->isSubclassOf('GrandParentClass'),
            $rc_child->isSubclassOf('BaseInterface'),
            $rc_parent->isSubclassOf('ParentClass'),
            $rc_interface->isSubclassOf('BaseInterface')
        ];
    "#).unwrap();

    assert_eq!(get_array_idx(&vm, &result, 0), Val::Bool(true));
    assert_eq!(get_array_idx(&vm, &result, 1), Val::Bool(true));
    assert_eq!(get_array_idx(&vm, &result, 2), Val::Bool(false));
    assert_eq!(get_array_idx(&vm, &result, 3), Val::Bool(true));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --test reflection_test`
Expected: FAIL on assertions for multi-level inheritance and interface checks.

### Task 2: Implement recursive subclass checking in ReflectionClass::isSubclassOf

**Files:**
- Modify: `src/builtins/reflection.rs`
- Test: `tests/reflection_test.rs`

**Step 1: Write minimal implementation**

```rust
let parent_sym = vm.context.interner.intern(&parent_name_bytes);

if class_name == parent_sym {
    return Ok(vm.arena.alloc(Val::Bool(false)));
}

let is_subclass = vm.is_subclass_of(class_name, parent_sym);
Ok(vm.arena.alloc(Val::Bool(is_subclass)))
```

**Step 2: Run test to verify it passes**

Run: `cargo test --test reflection_test`
Expected: PASS

### Task 3: Update implementation plan index

**Files:**
- Modify: `docs/IMPLEMENTATION_PLANS.md`

**Step 1: Add entry**

```markdown
- 2026-01-06: ReflectionClass::isSubclassOf multi-level inheritance (`docs/plans/2026-01-06-reflection-class-issubclassof.md`)
```

**Step 2: Commit**

```bash
git add tests/reflection_test.rs src/builtins/reflection.rs docs/IMPLEMENTATION_PLANS.md
git commit -m "Implement ReflectionClass::isSubclassOf recursion"
```
