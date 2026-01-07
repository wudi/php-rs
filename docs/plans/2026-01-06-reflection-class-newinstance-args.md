# ReflectionClass::newInstanceArgs Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement PHP-compatible `ReflectionClass::newInstanceArgs`, sharing object construction logic with `newInstance` and matching PHP argument handling.

**Architecture:** Extract a helper that creates the object and invokes the constructor, used by both `newInstance` and `newInstanceArgs`. `newInstanceArgs` will parse the optional array argument, collect values in insertion order (ignore keys), and pass those positional arguments into the shared helper.

**Tech Stack:** Rust, existing VM/object APIs in `src/builtins/reflection.rs`, tests in `tests/reflection_test.rs`.

### Task 1: Add tests for newInstanceArgs behavior

**Files:**
- Modify: `tests/reflection_test.rs`

**Step 1: Write the failing tests**

```rust
#[test]
fn test_reflection_class_new_instance_args_calls_constructor() {
    let (result, vm) = run_code_with_vm(r#"<?php
        class ArgsCtor {
            public int $a = 0;
            public int $b = 0;
            public function __construct(int $a, int $b) {
                $this->a = $a;
                $this->b = $b;
            }
        }
        $rc = new ReflectionClass('ArgsCtor');
        $obj = $rc->newInstanceArgs([1 => 7, 0 => 3]);
        return [$obj instanceof ArgsCtor, $obj->a, $obj->b];
    "#).unwrap();

    assert_eq!(get_array_idx(&vm, &result, 0), Val::Bool(true));
    assert_eq!(get_array_idx(&vm, &result, 1), Val::Int(3));
    assert_eq!(get_array_idx(&vm, &result, 2), Val::Int(7));
}

#[test]
fn test_reflection_class_new_instance_args_no_constructor_with_args_throws() {
    let result = run_code_with_vm(r#"<?php
        class NoCtorArgs {}
        $rc = new ReflectionClass('NoCtorArgs');
        $rc->newInstanceArgs([1]);
    "#);
    match result {
        Err(VmError::RuntimeError(msg)) => {
            assert!(
                msg.contains("Class NoCtorArgs does not have a constructor, so you cannot pass any constructor arguments"),
                "unexpected error: {msg}"
            );
        }
        Err(other) => panic!("Expected RuntimeError, got {:?}", other),
        Ok(_) => panic!("Expected error, got Ok"),
    }
}

#[test]
fn test_reflection_class_new_instance_args_zero_args_uses_empty_list() {
    let (result, vm) = run_code_with_vm(r#"<?php
        class ZeroArgs {
            public int $value = 11;
        }
        $rc = new ReflectionClass('ZeroArgs');
        $obj = $rc->newInstanceArgs();
        return [$obj instanceof ZeroArgs, $obj->value];
    "#).unwrap();

    assert_eq!(get_array_idx(&vm, &result, 0), Val::Bool(true));
    assert_eq!(get_array_idx(&vm, &result, 1), Val::Int(11));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --test reflection_test`
Expected: FAIL with `newInstanceArgs` returning null or argument handling errors.

### Task 2: Implement shared constructor helper and newInstanceArgs

**Files:**
- Modify: `src/builtins/reflection.rs`

**Step 1: Write minimal implementation**

```rust
fn reflection_class_new_instance_impl(
    vm: &mut VM,
    class_name: Symbol,
    class_def: &ClassDef,
    args: &[Handle],
) -> Result<Handle, String> {
    let class_name_bytes = lookup_symbol(vm, class_name);
    if class_def.is_abstract {
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

    let props = vm.collect_properties(class_def, PropertyCollectionMode::All)?;
    let obj = ObjectData {
        class_name,
        properties: props.default_values,
        dynamic_properties: HashSet::new(),
    };
    let obj_handle = vm.arena.alloc(Val::Object(Rc::new(obj)));

    let constructor_sym = vm.context.interner.intern(b"__construct");
    if let Some(constructor) = vm.find_method(class_def, constructor_sym) {
        if constructor.visibility != Visibility::Public {
            return Err(format!(
                "Access to non-public constructor of class {}",
                String::from_utf8_lossy(&class_name_bytes)
            ));
        }

        let method_name_handle = vm
            .arena
            .alloc(Val::String(Rc::new(b"__construct".to_vec())));
        let mut arr_data = ArrayData::new();
        arr_data.push(obj_handle);
        arr_data.push(method_name_handle);
        let callable_handle = vm.arena.alloc(Val::Array(Rc::new(arr_data)));

        let ctor_args: smallvec::SmallVec<[Handle; 8]> = args.iter().copied().collect();
        if let Err(err) = vm.call_callable(callable_handle, ctor_args) {
            return Err(match err {
                VmError::RuntimeError(msg) => msg,
                other => format!("Constructor invocation error: {:?}", other),
            });
        }
    } else if let Some(native_entry) = vm.find_native_method(class_name, constructor_sym) {
        if native_entry.visibility != Visibility::Public {
            return Err(format!(
                "Access to non-public constructor of class {}",
                String::from_utf8_lossy(&class_name_bytes)
            ));
        }

        let saved_this = vm.frames.last().and_then(|f| f.this);
        if let Some(frame) = vm.frames.last_mut() {
            frame.this = Some(obj_handle);
        }
        (native_entry.handler)(vm, args)?;
        if let Some(frame) = vm.frames.last_mut() {
            frame.this = saved_this;
        }
    } else if !args.is_empty() {
        return Err(format!(
            "Class {} does not have a constructor, so you cannot pass any constructor arguments",
            String::from_utf8_lossy(&class_name_bytes)
        ));
    }

    Ok(obj_handle)
}
```

Then update `reflection_class_new_instance` to gather `class_name` + `class_def` and call the helper, and implement `reflection_class_new_instance_args`:

```rust
pub fn reflection_class_new_instance_args(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() > 1 {
        return Err(format!(
            "ReflectionClass::newInstanceArgs() expects at most 1 argument, {} given",
            args.len()
        ));
    }

    let arg_values: Vec<Handle> = if let Some(arg) = args.first() {
        match &vm.arena.get(*arg).value {
            Val::Array(arr) => arr.map.values().copied().collect(),
            _ => {
                return Err(format!(
                    "ReflectionClass::newInstanceArgs() expects parameter 1 to be array, {} given",
                    vm.get_type_name(*arg)
                ))
            }
        }
    } else {
        Vec::new()
    };

    let class_name = get_reflection_class_name(vm)?;
    let class_def = get_class_def(vm, class_name)?;
    reflection_class_new_instance_impl(vm, class_name, class_def, &arg_values)
}
```

**Step 2: Run test to verify it passes**

Run: `cargo test --test reflection_test`
Expected: PASS

### Task 3: Verify overall test suite

**Files:**
- None

**Step 1: Run the full test suite**

Run: `cargo test`
Expected: PASS

### Task 4: Commit

**Step 1: Commit changes**

```bash
git add tests/reflection_test.rs src/builtins/reflection.rs
git commit -m "Implement ReflectionClass::newInstanceArgs"
```
