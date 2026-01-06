# ReflectionClass::newInstance Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement `ReflectionClass::newInstance` to match PHP semantics for constructor invocation, access checks, and argument handling.

**Architecture:** Use the VM’s object creation and method call helpers to allocate an instance, resolve the class constructor, enforce public visibility, and dispatch the constructor with variadic args. Mirror PHP’s error cases when passing args without a constructor and when constructor is non-public.

**Tech Stack:** Rust, php-rs VM, reflection builtins, Rust integration tests.

### Task 1: Add failing tests for ReflectionClass::newInstance

**Files:**
- Modify: `tests/reflection_test.rs`
- Test: `tests/reflection_test.rs`

**Step 1: Write the failing tests**

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
fn test_reflection_class_new_instance_calls_constructor() {
    let (result, vm) = common::run_code_with_vm(r#"<?php
        class CtorClass {
            public int $value = 0;
            public function __construct(int $v) { $this->value = $v; }
        }
        $rc = new ReflectionClass('CtorClass');
        $obj = $rc->newInstance(42);
        return [$obj instanceof CtorClass, $obj->value];
    "#).unwrap();

    assert_eq!(get_array_idx(&vm, &result, 0), Val::Bool(true));
    assert_eq!(get_array_idx(&vm, &result, 1), Val::Int(42));
}

#[test]
fn test_reflection_class_new_instance_no_constructor_with_args_throws() {
    let (result, _vm) = common::run_code_with_vm(r#"<?php
        class NoCtor {}
        $rc = new ReflectionClass('NoCtor');
        try {
            $rc->newInstance(1);
            return false;
        } catch (ReflectionException $e) {
            return true;
        }
    "#).unwrap();

    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_class_new_instance_non_public_constructor_throws() {
    let (result, _vm) = common::run_code_with_vm(r#"<?php
        class PrivateCtor {
            private function __construct() {}
        }
        $rc = new ReflectionClass('PrivateCtor');
        try {
            $rc->newInstance();
            return false;
        } catch (ReflectionException $e) {
            return true;
        }
    "#).unwrap();

    assert_eq!(result, Val::Bool(true));
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test --test reflection_test`
Expected: FAIL because `ReflectionClass::newInstance` returns null and does not call constructors.

### Task 2: Implement ReflectionClass::newInstance

**Files:**
- Modify: `src/builtins/reflection.rs`
- Modify: `src/vm/engine.rs` (if needed to access constructor lookup/call)
- Test: `tests/reflection_test.rs`

**Step 1: Implement minimal behavior**

```rust
let class_name = get_reflection_class_name(vm)?;
let class_def = get_class_def(vm, class_name)?;
let class_name_bytes = lookup_symbol(vm, class_name);

let obj_handle = create_object_with_properties(vm, &class_name_bytes, &[])?;

let constructor = class_def.methods.get(&vm.context.interner.intern(b"__construct"));
if let Some(method) = constructor {
    if method.visibility != Visibility::Public {
        return Err(format!("Access to non-public constructor of class {}", String::from_utf8_lossy(&class_name_bytes)));
    }

    // Call constructor with provided args
    call_method(vm, obj_handle, method, args)?;
} else if !args.is_empty() {
    return Err(format!("Class {} does not have a constructor, so you cannot pass any constructor arguments", String::from_utf8_lossy(&class_name_bytes)));
}

Ok(obj_handle)
```

**Step 2: Run tests to verify they pass**

Run: `cargo test --test reflection_test`
Expected: PASS

### Task 3: Update implementation plan index

**Files:**
- Modify: `docs/IMPLEMENTATION_PLANS.md`

**Step 1: Add entry**

```markdown
- 2026-01-06: ReflectionClass::newInstance implementation (`docs/plans/2026-01-06-reflection-class-newinstance.md`)
```

**Step 2: Commit**

```bash
git add tests/reflection_test.rs src/builtins/reflection.rs src/vm/engine.rs docs/IMPLEMENTATION_PLANS.md
git commit -m "Implement ReflectionClass::newInstance"
```
