# Class Final Support Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add `final` class tracking in `ClassDef`, enforce inheritance rules, and surface final status for native classes via reflection.

**Architecture:** Introduce an `is_final` flag in `ClassDef` and `NativeClassDef`, populate it from class modifiers or native registry metadata, and wire it into reflection APIs. Add inheritance validation during class finalization to reject extending a final parent.

**Tech Stack:** Rust, php-rs compiler/VM, integration tests in `tests/`.

**Skills:** @superpowers:test-driven-development @superpowers:executing-plans

### Task 1: Track final on user-defined classes and expose via ReflectionClass

**Files:**
- Modify: `src/runtime/context.rs`
- Modify: `src/compiler/emitter.rs`
- Modify: `src/vm/opcode.rs`
- Modify: `src/vm/engine.rs`
- Modify: `src/vm/class_resolution.rs`
- Modify: `src/builtins/reflection.rs`
- Test: `tests/reflection_test.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn test_reflection_class_is_final() {
    let result = run_php(r#"<?php
        final class FinalClass {}
        $rc = new ReflectionClass('FinalClass');
        $names = Reflection::getModifierNames($rc->getModifiers());
        return $rc->isFinal() && in_array('final', $names, true);
    "#);

    assert_eq!(result, Val::Bool(true));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --test reflection_test test_reflection_class_is_final`
Expected: FAIL (isFinal returns false and/or modifiers missing final)

**Step 3: Write minimal implementation**

- Add `is_final: bool` to `ClassDef` in `src/runtime/context.rs` and initialize to `false` everywhere `ClassDef` is constructed (VM class creation, class resolution tests).
- Add a new opcode `MarkFinal(Symbol)` in `src/vm/opcode.rs` (parallel to `MarkAbstract`).
- In `src/compiler/emitter.rs`, when emitting a class definition, detect `final` modifier and emit `OpCode::MarkFinal(class_sym)`.
- In `src/vm/engine.rs`, handle `OpCode::MarkFinal` by setting `class_def.is_final = true`.
- In `src/builtins/reflection.rs`, return `class_def.is_final` from `ReflectionClass::isFinal()` and include `IS_FINAL` in `ReflectionClass::getModifiers()`.

**Step 4: Run test to verify it passes**

Run: `cargo test --test reflection_test test_reflection_class_is_final`
Expected: PASS

**Step 5: Commit**

```bash
git add src/runtime/context.rs src/compiler/emitter.rs src/vm/opcode.rs src/vm/engine.rs src/vm/class_resolution.rs src/builtins/reflection.rs tests/reflection_test.rs
git commit -m "Add final class tracking for reflection"
```

### Task 2: Enforce final class inheritance

**Files:**
- Modify: `src/vm/engine.rs`
- Test: `tests/classes.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn test_extending_final_class_errors() {
    run_code_expect_error(
        r#"<?php
        final class Base {}
        class Child extends Base {}
        "#,
        "cannot extend final class Base",
    );
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --test classes test_extending_final_class_errors`
Expected: FAIL (no error thrown)

**Step 3: Write minimal implementation**

- In `src/vm/engine.rs` within `OpCode::FinalizeClass`, if `class_def.parent` is set, resolve the parent class definition and return a `VmError::RuntimeError` when `parent_def.is_final` is true.
- Use an error message that matches the test (e.g., `"Class Child cannot extend final class Base"` or adjust the test string accordingly).

**Step 4: Run test to verify it passes**

Run: `cargo test --test classes test_extending_final_class_errors`
Expected: PASS

**Step 5: Commit**

```bash
git add src/vm/engine.rs tests/classes.rs
git commit -m "Enforce final class inheritance"
```

### Task 3: Track final on native classes

**Files:**
- Modify: `src/runtime/registry.rs`
- Modify: `src/runtime/context.rs`
- Modify: `src/runtime/core_extension.rs`
- Modify: `src/builtins/reflection.rs`
- Modify: `src/builtins/zip/mod.rs`
- Modify: `src/builtins/pdo/mod.rs`
- Modify: `src/runtime/date_extension.rs`
- Modify: `src/runtime/openssl_extension.rs`
- Modify: `src/runtime/zlib_extension.rs`
- Test: `tests/reflection_test.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn test_reflection_class_is_final_native() {
    let result = run_php(r#"<?php
        $rc = new ReflectionClass('Closure');
        return $rc->isFinal();
    "#);

    assert_eq!(result, Val::Bool(true));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --test reflection_test test_reflection_class_is_final_native`
Expected: FAIL (native classes not marked final)

**Step 3: Write minimal implementation**

- Add `is_final: bool` to `NativeClassDef` in `src/runtime/registry.rs`.
- Update all `NativeClassDef` initializations to include `is_final: false` by default.
- Set `is_final: true` for native final classes (at minimum `Closure` and `Generator`) in `src/runtime/core_extension.rs`. Confirm in `$PHP_SRC_PATH` which classes are final before setting additional flags.
- In `src/runtime/context.rs`, propagate `native_class.is_final` into `ClassDef::is_final` during `materialize_extension_classes`.

**Step 4: Run test to verify it passes**

Run: `cargo test --test reflection_test test_reflection_class_is_final_native`
Expected: PASS

**Step 5: Commit**

```bash
git add src/runtime/registry.rs src/runtime/context.rs src/runtime/core_extension.rs src/builtins/reflection.rs src/builtins/zip/mod.rs src/builtins/pdo/mod.rs src/runtime/date_extension.rs src/runtime/openssl_extension.rs src/runtime/zlib_extension.rs tests/reflection_test.rs
git commit -m "Mark native final classes"
```
