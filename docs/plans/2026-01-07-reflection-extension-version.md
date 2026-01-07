# ReflectionExtension Version Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement ReflectionExtension::getVersion and validate ReflectionExtension::__construct against loaded extensions, matching PHP 8.5 behavior.

**Architecture:** Add a case-insensitive extension info lookup in `ExtensionRegistry`, use it in ReflectionExtension::__construct to throw ReflectionException for unknown extensions and to store the canonical name, then implement getVersion by looking up the stored name and returning `null` for empty versions.

**Tech Stack:** Rust, internal runtime registry, Reflection builtins, Rust tests

### Task 1: Add failing tests for ReflectionExtension version and constructor validation

**Files:**
- Create: `tests/reflection_extension_version.rs`
- Modify: `tests/common/mod.rs` (only if helper needed; prefer reuse)

**Step 1: Write the failing test**

```rust
mod common;
use common::run_code_capture_output;

#[test]
fn reflection_extension_get_version_returns_string() {
    let script = r#"<?php
        $ext = new ReflectionExtension('reflection');
        var_dump($ext->getVersion());
    "#;
    let (_val, output) = run_code_capture_output(script).expect("Execution failed");
    assert!(output.contains("string("));
}

#[test]
fn reflection_extension_construct_is_case_insensitive() {
    let script = r#"<?php
        $ext = new ReflectionExtension('Reflection');
        var_dump($ext->getName());
    "#;
    let (_val, output) = run_code_capture_output(script).expect("Execution failed");
    assert!(output.contains("string(10) \"Reflection\""));
}

#[test]
fn reflection_extension_construct_unknown_throws() {
    let script = r#"<?php
        try {
            new ReflectionExtension('no_such_extension');
        } catch (ReflectionException $e) {
            echo $e->getMessage();
        }
    "#;
    let (_val, output) = run_code_capture_output(script).expect("Execution failed");
    assert!(output.contains("Extension \"no_such_extension\" does not exist"));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --test reflection_extension_version`
Expected: FAIL with getVersion returning NULL and constructor not throwing.

### Task 2: Add case-insensitive extension lookup in registry

**Files:**
- Modify: `src/runtime/registry.rs`

**Step 1: Write minimal implementation**

```rust
pub fn get_extension_info_by_name_ci(&self, name: &str) -> Option<ExtensionInfo> {
    for (ext_name, &index) in &self.extension_map {
        if ext_name.eq_ignore_ascii_case(name) {
            if let Some(ext) = self.extensions.get(index) {
                return Some(ext.info());
            }
        }
    }
    None
}
```

**Step 2: Run test to verify it still fails**

Run: `cargo test --test reflection_extension_version`
Expected: still FAIL (ReflectionExtension not using new lookup yet).

### Task 3: Implement ReflectionExtension constructor validation

**Files:**
- Modify: `src/builtins/reflection.rs`

**Step 1: Write minimal implementation**

```rust
let ext_name_str = std::str::from_utf8(ext_name_bytes)
    .map_err(|_| "ReflectionExtension::__construct() expects parameter 1 to be string".to_string())?;

let info = vm
    .context
    .engine
    .registry
    .get_extension_info_by_name_ci(ext_name_str);

let info = match info {
    Some(info) => info,
    None => {
        return Err(format!("Extension \"{}\" does not exist", ext_name_str));
    }
};

let ext_name_handle = vm.arena.alloc(Val::String(Rc::new(info.name.as_bytes().to_vec())));
```

**Step 2: Run test to verify it still fails**

Run: `cargo test --test reflection_extension_version`
Expected: FAIL only on getVersion returning NULL.

### Task 4: Implement ReflectionExtension::getVersion

**Files:**
- Modify: `src/builtins/reflection.rs`

**Step 1: Write minimal implementation**

```rust
let data = get_reflection_extension_data(vm)?;
let name_bytes = lookup_symbol(vm, data.name);
let name_str = std::str::from_utf8(name_bytes)
    .map_err(|_| "ReflectionExtension::getVersion() extension name is invalid".to_string())?;

if let Some(info) = vm
    .context
    .engine
    .registry
    .get_extension_info_by_name_ci(name_str)
{
    if info.version.is_empty() {
        return Ok(vm.arena.alloc(Val::Null));
    }
    return Ok(vm
        .arena
        .alloc(Val::String(Rc::new(info.version.as_bytes().to_vec()))));
}

Ok(vm.arena.alloc(Val::Null))
```

**Step 2: Run test to verify it passes**

Run: `cargo test --test reflection_extension_version`
Expected: PASS.

### Task 5: Refactor/cleanup and finalize

**Files:**
- Modify: `docs/IMPLEMENTATION_PLANS.md`

**Step 1: Add plan entry**

Add a bullet for `2026-01-07-reflection-extension-version.md` in the reflection section.

**Step 2: Run formatting if needed**

Run: `cargo fmt`
Expected: no diff or rustfmt adjustments only.

**Step 3: Commit**

```bash
git add tests/reflection_extension_version.rs src/runtime/registry.rs src/builtins/reflection.rs docs/IMPLEMENTATION_PLANS.md docs/plans/2026-01-07-reflection-extension-version.md
git commit -m "Implement ReflectionExtension version lookup"
```
