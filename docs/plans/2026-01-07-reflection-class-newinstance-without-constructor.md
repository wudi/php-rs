# ReflectionClass::newInstanceWithoutConstructor Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement `ReflectionClass::newInstanceWithoutConstructor()` to match PHP behavior and add tests for constructor bypass and error cases.

**Architecture:** Add a small helper in `src/builtins/reflection.rs` that enforces the internal-final guard and delegates to the VM's normal object instantiation path without invoking constructors. Tests in `tests/reflection_test.rs` will mirror PHP behavior for user classes, enums, and abstract/interface/trait cases.

**Tech Stack:** Rust, existing VM/object helpers, Rust test harness (`cargo test`).

### Task 1: Add failing tests for newInstanceWithoutConstructor

**Files:**
- Modify: `tests/reflection_test.rs`

**Step 1: Write failing tests**

Add tests covering constructor bypass and instantiation errors:

```rust
#[test]
fn reflection_class_new_instance_without_constructor_does_not_call_ctor() {
    let output = run_php(r#"
        class Foo {
            public int $x = 0;
            public function __construct() { $this->x = 42; }
        }
        $rc = new ReflectionClass(Foo::class);
        $obj = $rc->newInstanceWithoutConstructor();
        var_dump($obj instanceof Foo);
        var_dump($obj->x);
    "#);
    assert_eq!(output, "bool(true)\nint(0)\n");
}

#[test]
fn reflection_class_new_instance_without_constructor_enum_error() {
    let output = run_php(r#"
        enum Foo {}
        $rc = new ReflectionClass(Foo::class);
        try {
            $rc->newInstanceWithoutConstructor();
        } catch (Error $e) {
            echo $e->getMessage(), \"\n\";
        }
    "#);
    assert_eq!(output, "Cannot instantiate enum Foo\n");
}

#[test]
fn reflection_class_new_instance_without_constructor_abstract_interface_trait_errors() {
    let output = run_php(r#"
        abstract class A {}
        interface I {}
        trait T {}
        foreach ([A::class, I::class, T::class] as $name) {
            $rc = new ReflectionClass($name);
            try {
                $rc->newInstanceWithoutConstructor();
            } catch (Error $e) {
                echo $e->getMessage(), \"\n\";
            }
        }
    "#);
    assert_eq!(
        output,
        "Cannot instantiate abstract class A\nCannot instantiate interface I\nCannot instantiate trait T\n"
    );
}

#[test]
fn reflection_class_new_instance_without_constructor_internal_final_guard() {
    let output = run_php(r#"
        $rc = new ReflectionClass(Generator::class);
        try {
            $rc->newInstanceWithoutConstructor();
        } catch (ReflectionException $e) {
            echo $e->getMessage(), \"\n\";
        }
    "#);
    assert_eq!(
        output,
        "Class Generator is an internal class marked as final that cannot be instantiated without invoking its constructor\n"
    );
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --test reflection_test reflection_class_new_instance_without_constructor_does_not_call_ctor`
Expected: FAIL due to missing implementation or wrong behavior.

**Step 3: Commit the failing test**

```bash
git add tests/reflection_test.rs
git commit -m "Add failing tests for ReflectionClass::newInstanceWithoutConstructor"
```

### Task 2: Implement newInstanceWithoutConstructor behavior

**Files:**
- Modify: `src/builtins/reflection.rs`
- Modify: `src/runtime/context.rs`
- Modify: `src/vm/class_resolution.rs`
- Modify: `src/vm/engine.rs`

**Step 1: Write minimal implementation**

Add an internal flag to `ClassDef` and set it for native classes. For user-defined classes in `src/vm/engine.rs` and class-resolution helpers, initialize the flag as `false`. In `materialize_extension_classes`, set it to `true`. Update `reflection_class_is_internal` to use the flag. Treat internal+final as the PHP guard (we do not track custom allocators yet).

Then implement a helper mirroring PHP behavior:

```rust
fn reflection_class_new_instance_without_constructor_impl(
    vm: &mut VM,
    class_name: Symbol,
    class_def: &ClassDef,
) -> Result<Handle, String> {
    let class_name_bytes = lookup_symbol(vm, class_name).to_vec();

    if class_def.is_abstract && !class_def.is_interface {
        return Err(format!(
            "Cannot instantiate abstract class {}",
            String::from_utf8_lossy(&class_name_bytes)
        ));
    }
    if class_def.is_interface {
        return Err(format!(
            "Cannot instantiate interface {}",
            String::from_utf8_lossy(&class_name_bytes)
        ));
    }
    if class_def.is_trait {
        return Err(format!(
            "Cannot instantiate trait {}",
            String::from_utf8_lossy(&class_name_bytes)
        ));
    }
    if class_def.is_enum {
        return Err(format!(
            "Cannot instantiate enum {}",
            String::from_utf8_lossy(&class_name_bytes)
        ));
    }

    if class_def.is_internal && class_def.is_final {
        return Err(format!(
            "Class {} is an internal class marked as final that cannot be instantiated without invoking its constructor",
            String::from_utf8_lossy(&class_name_bytes)
        ));
    }

    let properties = vm.collect_properties(class_name, PropertyCollectionMode::All);
    let obj_data = ObjectData {
        class: class_name,
        properties,
        internal: None,
        dynamic_properties: HashSet::new(),
    };
    let payload_handle = vm.arena.alloc(Val::ObjPayload(obj_data));
    Ok(vm.arena.alloc(Val::Object(payload_handle)))
}
```

Then use it in `reflection_class_new_instance_without_constructor`.

**Step 2: Run test to verify it passes**

Run: `cargo test --test reflection_test reflection_class_new_instance_without_constructor_does_not_call_ctor`
Expected: PASS.

**Step 3: Commit**

```bash
git add src/builtins/reflection.rs
git commit -m "Implement ReflectionClass::newInstanceWithoutConstructor"
```

### Task 3: Verify full reflection test suite

**Files:**
- None

**Step 1: Run full reflection tests**

Run: `cargo test --test reflection_test`
Expected: PASS.

**Step 2: Commit any fixes**

```bash
git add tests/reflection_test.rs src/builtins/reflection.rs
git commit -m "Fix ReflectionClass::newInstanceWithoutConstructor tests"
```
