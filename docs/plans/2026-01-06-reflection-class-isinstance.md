# ReflectionClass::isInstance Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make `ReflectionClass::isInstance()` match PHP `instanceof` semantics (parent chain + interfaces).

**Architecture:** Expose a VM helper that checks instance-of semantics using the existing `is_subclass_of` logic (class equality + parent and interface traversal). Update reflection to call that helper after extracting the object’s class symbol. Add reflection tests for parent and interface cases.

**Tech Stack:** Rust, existing VM (`src/vm/engine.rs`), reflection builtins (`src/builtins/reflection.rs`), integration tests (`tests/reflection_test.rs`).

### Task 1: Add failing reflection tests for parent and interface cases

**Files:**
- Modify: `tests/reflection_test.rs`

**Step 1: Write the failing tests**

Append tests near existing `test_reflection_class_is_instance`:

```rust
#[test]
fn test_reflection_class_is_instance_parent_chain() {
    let result = run_php(r#"<?php
        class ParentClass {}
        class ChildClass extends ParentClass {}

        $obj = new ChildClass();
        $rc = new ReflectionClass('ParentClass');
        return $rc->isInstance($obj);
    "#);

    assert_eq!(result, Val::Bool(true));
}

#[test]
fn test_reflection_class_is_instance_interface() {
    let result = run_php(r#"<?php
        interface TestInterface {}
        class ImplClass implements TestInterface {}

        $obj = new ImplClass();
        $rc = new ReflectionClass('TestInterface');
        return $rc->isInstance($obj);
    "#);

    assert_eq!(result, Val::Bool(true));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --test reflection_test`

Expected: FAIL at the new tests (returns false).

### Task 2: Implement VM helper for instance-of semantics

**Files:**
- Modify: `src/vm/engine.rs`

**Step 1: Update helper to be callable by reflection**

Locate the existing helper:

```rust
fn is_instance_of_class(&self, obj_class: Symbol, target_class: Symbol) -> bool {
    if obj_class == target_class {
        return true;
    }

    // Check parent classes
    if let Some(class_def) = self.context.classes.get(&obj_class) {
        if let Some(parent) = class_def.parent {
            return self.is_instance_of_class(parent, target_class);
        }
    }

    false
}
```

Replace it with a `pub(crate)` method that delegates to `is_subclass_of` (which includes interfaces):

```rust
pub(crate) fn is_instance_of_class(&self, obj_class: Symbol, target_class: Symbol) -> bool {
    self.is_subclass_of(obj_class, target_class)
}
```

**Step 2: Run tests to confirm behavior**

Run: `cargo test --test reflection_test`

Expected: still failing until reflection calls the helper.

### Task 3: Update reflection to call VM helper

**Files:**
- Modify: `src/builtins/reflection.rs`

**Step 1: Implement full instanceof in ReflectionClass::isInstance**

Update `reflection_class_is_instance` to call `vm.is_instance_of_class(obj_class_sym, class_name)` after extracting `obj_class_sym`.

**Step 2: Run tests**

Run: `cargo test --test reflection_test`

Expected: PASS (including new tests).

### Task 4: Update plan index

**Files:**
- Modify: `docs/IMPLEMENTATION_PLANS.md`

**Step 1: Add entry**

Add a line:

```
- 2026-01-06: ReflectionClass::isInstance instanceof semantics (`docs/plans/2026-01-06-reflection-class-isinstance.md`)
```

### Task 5: Full test run

**Step 1: Run full test suite**

Run: `cargo test`

Expected: PASS (warnings ok).

### Task 6: Commit

**Step 1: Commit changes**

```bash
git add tests/reflection_test.rs src/vm/engine.rs src/builtins/reflection.rs docs/IMPLEMENTATION_PLANS.md docs/plans/2026-01-06-reflection-class-isinstance.md
git commit -m "Implement ReflectionClass::isInstance instanceof behavior"
```
