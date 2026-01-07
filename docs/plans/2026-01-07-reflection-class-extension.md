# ReflectionClass Extension Tracking Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Track owning extension for internal classes so `ReflectionClass::getExtension()` and `getExtensionName()` match PHP 8.5.

**Architecture:** Attach extension ownership to native class registrations, copy it into `ClassDef` during materialization, and have reflection read the field directly. Extension names should match PHP module names exactly (case-sensitive).

**Tech Stack:** Rust, existing extension registry (`src/runtime/registry.rs`), class metadata (`src/runtime/context.rs`), reflection builtins (`src/builtins/reflection.rs`).

### Task 1: Add extension ownership fields to class metadata

**Files:**
- Modify: `src/runtime/registry.rs`
- Modify: `src/runtime/context.rs`

**Step 1: Write the failing test**

Create `tests/reflection_class_extension.rs`:

```rust
use php_rs::engine::Engine;

#[test]
fn reflection_class_extension_for_internal_class() {
    let engine = Engine::builder().with_core_extensions().build().unwrap();
    let mut vm = engine.start_vm();
    let script = br#"
        $rc = new ReflectionClass('ReflectionClass');
        var_dump($rc->getExtensionName());
    "#;
    let output = vm.exec(script).unwrap();
    assert!(output.contains("string(10) \"Reflection\""));
}

#[test]
fn reflection_class_extension_for_user_class() {
    let engine = Engine::builder().with_core_extensions().build().unwrap();
    let mut vm = engine.start_vm();
    let script = br#"
        class Foo {}
        $rc = new ReflectionClass('Foo');
        var_dump($rc->getExtensionName());
    "#;
    let output = vm.exec(script).unwrap();
    assert!(output.contains("bool(false)"));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --test reflection_class_extension`
Expected: FAIL because `getExtensionName()` currently returns `false` for internal classes.

**Step 3: Write minimal implementation**

Update `NativeClassDef` to carry extension name:

```rust
pub struct NativeClassDef {
    pub name: Vec<u8>,
    pub parent: Option<Vec<u8>>,
    pub is_interface: bool,
    pub is_trait: bool,
    pub is_final: bool,
    pub interfaces: Vec<Vec<u8>>,
    pub methods: HashMap<Vec<u8>, NativeMethodEntry>,
    pub constants: HashMap<Vec<u8>, (Val, Visibility)>,
    pub constructor: Option<NativeHandler>,
    pub extension_name: Option<Vec<u8>>,
}
```

Update `ClassDef` to hold interned extension name:

```rust
pub struct ClassDef {
    // ... existing fields
    pub extension_name: Option<Symbol>,
}
```

**Step 4: Run test to verify it still fails**

Run: `cargo test --test reflection_class_extension`
Expected: FAIL (field exists but not wired).

**Step 5: Commit**

```bash
git add src/runtime/registry.rs src/runtime/context.rs tests/reflection_class_extension.rs
git commit -m "Add extension fields to class metadata"
```

### Task 2: Tag native classes with extension name during registration

**Files:**
- Modify: `src/runtime/registry.rs`
- Modify: `src/runtime/context.rs`

**Step 1: Write the failing test**

Extend `tests/reflection_class_extension.rs`:

```rust
#[test]
fn reflection_class_extension_object() {
    let engine = Engine::builder().with_core_extensions().build().unwrap();
    let mut vm = engine.start_vm();
    let script = br#"
        $rc = new ReflectionClass('ReflectionClass');
        $ext = $rc->getExtension();
        var_dump($ext instanceof ReflectionExtension);
        var_dump($ext->getName());
    "#;
    let output = vm.exec(script).unwrap();
    assert!(output.contains("bool(true)"));
    assert!(output.contains("string(10) \"Reflection\""));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --test reflection_class_extension`
Expected: FAIL because `getExtension()` returns null.

**Step 3: Write minimal implementation**

Tag classes in `ExtensionRegistry::register_extension`:

```rust
pub fn register_extension(&mut self, extension: Box<dyn Extension>) -> Result<(), String> {
    let info = extension.info();
    // ... existing checks
    let extension_name = info.name.as_bytes().to_vec();
    match extension.module_init(self) {
        ExtensionResult::Success => {
            self.pending_extension_name = Some(extension_name);
            // existing insert
        }
        // ...
    }
}
```

Then update `register_class` to attach the pending name when present:

```rust
pub fn register_class(&mut self, mut class: NativeClassDef) {
    if class.extension_name.is_none() {
        class.extension_name = self.pending_extension_name.clone();
    }
    self.classes.insert(class.name.clone(), class);
}
```

Propagate into `ClassDef` during materialization:

```rust
let extension_name = native_class
    .extension_name
    .as_ref()
    .map(|name| self.interner.intern(name));

self.classes.insert(
    class_sym,
    ClassDef {
        // ...
        extension_name,
        // ...
    },
);
```

**Step 4: Run test to verify it passes**

Run: `cargo test --test reflection_class_extension`
Expected: PASS

**Step 5: Commit**

```bash
git add src/runtime/registry.rs src/runtime/context.rs tests/reflection_class_extension.rs
git commit -m "Tag native classes with extension name"
```

### Task 3: Implement ReflectionClass::getExtension/getExtensionName

**Files:**
- Modify: `src/builtins/reflection.rs`

**Step 1: Write the failing test**

Add a case for user-defined class still returning false:

```rust
#[test]
fn reflection_class_extension_user_class() {
    let engine = Engine::builder().with_core_extensions().build().unwrap();
    let mut vm = engine.start_vm();
    let script = br#"
        class Foo {}
        $rc = new ReflectionClass('Foo');
        var_dump($rc->getExtension());
        var_dump($rc->getExtensionName());
    "#;
    let output = vm.exec(script).unwrap();
    assert!(output.contains("NULL"));
    assert!(output.contains("bool(false)"));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --test reflection_class_extension`
Expected: FAIL because `getExtensionName()` always returns false and `getExtension()` returns null.

**Step 3: Write minimal implementation**

Use the stored extension name in `ClassDef`:

```rust
pub fn reflection_class_get_extension(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let class_name = get_reflection_class_name(vm)?;
    let class_def = get_class_def(vm, class_name)?;

    if let Some(ext_sym) = class_def.extension_name {
        let ext_name = lookup_symbol(vm, ext_sym).to_vec();
        return create_object_with_properties(
            vm,
            b"ReflectionExtension",
            &[(b"name", Val::String(Rc::new(ext_name)))],
        );
    }

    Ok(vm.arena.alloc(Val::Null))
}

pub fn reflection_class_get_extension_name(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let class_name = get_reflection_class_name(vm)?;
    let class_def = get_class_def(vm, class_name)?;

    if let Some(ext_sym) = class_def.extension_name {
        let ext_name = lookup_symbol(vm, ext_sym).to_vec();
        return Ok(vm.arena.alloc(Val::String(Rc::new(ext_name))));
    }

    Ok(vm.arena.alloc(Val::Bool(false)))
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test --test reflection_class_extension`
Expected: PASS

**Step 5: Commit**

```bash
git add src/builtins/reflection.rs tests/reflection_class_extension.rs
git commit -m "Implement ReflectionClass extension lookups"
```

### Task 4: Ensure extension names match PHP module casing

**Files:**
- Modify: `src/runtime/*_extension.rs`

**Step 1: Write the failing test**

Update the test to check exact casing for a couple of builtins:

```rust
#[test]
fn reflection_class_extension_name_casing() {
    let engine = Engine::builder().with_core_extensions().build().unwrap();
    let mut vm = engine.start_vm();
    let script = br#"
        $rc = new ReflectionClass('ReflectionClass');
        var_dump($rc->getExtensionName());
    "#;
    let output = vm.exec(script).unwrap();
    assert!(output.contains("string(10) \"Reflection\""));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --test reflection_class_extension`
Expected: FAIL if extension names are lowercased.

**Step 3: Write minimal implementation**

Update `ExtensionInfo.name` values to match PHP 8.5 module names, for example:

```rust
ExtensionInfo { name: "Core", ... }
ExtensionInfo { name: "Reflection", ... }
ExtensionInfo { name: "PDO", ... }
```

**Step 4: Run test to verify it passes**

Run: `cargo test --test reflection_class_extension`
Expected: PASS

**Step 5: Commit**

```bash
git add src/runtime/*_extension.rs
git commit -m "Align extension module names with PHP"
```

