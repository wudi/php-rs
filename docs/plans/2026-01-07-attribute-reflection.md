# Attribute Reflection Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement PHP 8.5-compatible attributes end-to-end (parsing, validation, runtime storage, reflection, and instantiation).

**Architecture:** Parse attributes into AST, compile to constant attribute payloads, attach to runtime metadata via new opcodes, and expose through Reflection. Enforce target/repeatable rules per PHP 8.5, with user-attribute validation deferred to `ReflectionAttribute::newInstance()`.

**Tech Stack:** Rust, php-rs parser/emitter/VM, reflection builtins, PHP 8.5 source (`~/php-src`).

---

### Task 1: Add runtime attribute metadata types

**Files:**
- Create: `src/runtime/attributes.rs`
- Modify: `src/runtime/context.rs`
- Modify: `src/builtins/reflection.rs`
- Test: `tests/reflection_attributes.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn test_reflection_class_get_attributes_basic() {
    let code = r#"<?php
#[Example(1, name: "x")]
class Foo {}
$ref = new ReflectionClass(Foo::class);
$attrs = $ref->getAttributes();
echo count($attrs), "\n";
echo $attrs[0]->getName(), "\n";
var_dump($attrs[0]->getArguments());
"#;
    let (_, output) = crate::common::run_code_capture_output(code).unwrap();
    assert!(output.contains("1\n"));
    assert!(output.contains("Example\n"));
    assert!(output.contains("array(2)"));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --test reflection_attributes test_reflection_class_get_attributes_basic`
Expected: FAIL with missing attribute handling/empty array.

**Step 3: Write minimal implementation**

```rust
// src/runtime/attributes.rs
pub struct AttributeArg { pub name: Option<Symbol>, pub value: Val }
pub struct AttributeInstance { pub name: Symbol, pub lc_name: Symbol, pub args: Vec<AttributeArg>, pub target: u32, pub offset: u32, pub line: u32, pub validation_error: Option<String> }
pub struct AttributeClassInfo { pub targets: u32, pub is_repeatable: bool }
```

Add `attributes: Vec<AttributeInstance>` to:
- `ClassDef`
- `MethodEntry`
- `PropertyEntry`
- `ParameterInfo`

Add metadata for constants and functions:
- Replace `ClassDef.constants: HashMap<Symbol, (Val, Visibility)>` with a struct containing `value`, `visibility`, `attributes`.
- Replace `RequestContext.constants: HashMap<Symbol, Val>` with a struct containing `value` and `attributes`.
- Add `user_function_metadata: HashMap<Symbol, FuncMeta { attributes }>` or extend `UserFunc`.

Update reflection helpers to read attributes from these fields (return empty vec until fully wired).

**Step 4: Run test to verify it passes**

Run: `cargo test --test reflection_attributes test_reflection_class_get_attributes_basic`
Expected: still failing (pipeline not wired) — acceptable for this task, since storage is added but not populated.

**Step 5: Commit**

```bash
git add src/runtime/attributes.rs src/runtime/context.rs src/builtins/reflection.rs tests/reflection_attributes.rs
git commit -m "Add runtime attribute metadata scaffolding"
```

---

### Task 2: Compile-time attribute argument validation

**Files:**
- Modify: `src/parser/parser/attributes.rs`
- Modify: `src/compiler/emitter.rs`
- Modify: `src/parser/parser/expr.rs`
- Test: `tests/reflection_attributes.rs`

**Step 1: Write the failing tests**

```rust
#[test]
#[should_panic]
fn test_attribute_disallows_unpacking() {
    let code = r#"<?php
#[Example(...[1,2])]
class Foo {}
"#;
    let _ = crate::common::run_code(code);
}

#[test]
#[should_panic]
fn test_attribute_disallows_closure_argument() {
    let code = r#"<?php
#[Example(fn() => 1)]
class Foo {}
"#;
    let _ = crate::common::run_code(code);
}

#[test]
#[should_panic]
fn test_attribute_disallows_positional_after_named() {
    let code = r#"<?php
#[Example(name: "x", 1)]
class Foo {}
"#;
    let _ = crate::common::run_code(code);
}

#[test]
#[should_panic]
fn test_attribute_disallows_duplicate_named() {
    let code = r#"<?php
#[Example(name: "x", name: "y")]
class Foo {}
"#;
    let _ = crate::common::run_code(code);
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test --test reflection_attributes test_attribute_disallows_unpacking`
Expected: FAIL (no validation yet).

**Step 3: Write minimal implementation**

- Detect unpack and closure expressions in `parse_attributes()` and emit parser errors consistent with PHP 8.5.
- Track named argument usage and duplicate names in attribute argument lists.
- Keep validation consistent with PHP behavior in `Zend/zend_compile.c` (`zend_compile_attributes`).

**Step 4: Run tests to verify they pass**

Run: `cargo test --test reflection_attributes test_attribute_disallows_unpacking`
Expected: PASS

**Step 5: Commit**

```bash
git add src/parser/parser/attributes.rs src/parser/parser/expr.rs src/compiler/emitter.rs tests/reflection_attributes.rs
git commit -m "Validate attribute arguments during parsing"
```

---

### Task 3: Emit attribute opcodes and attach to runtime metadata

**Files:**
- Modify: `src/compiler/emitter.rs`
- Modify: `src/vm/opcode.rs`
- Modify: `src/vm/engine.rs`
- Modify: `src/runtime/context.rs`
- Test: `tests/reflection_attributes.rs`

**Step 1: Write the failing tests**

```rust
#[test]
fn test_reflection_function_method_property_param_const_attributes() {
    let code = r#"<?php
#[Example]
function foo(#[ExampleParam] $x) {}
class Foo {
    #[ExampleProp]
    public int $x;
    #[ExampleConst]
    public const C = 1;
    #[ExampleMethod]
    public function bar(#[ExampleParam] $y) {}
}
$rf = new ReflectionFunction('foo');
$rm = new ReflectionMethod(Foo::class, 'bar');
$rp = new ReflectionProperty(Foo::class, 'x');
$rc = new ReflectionClassConstant(Foo::class, 'C');
$attrs = [$rf->getAttributes(), $rm->getAttributes(), $rp->getAttributes(), $rc->getAttributes()];
foreach ($attrs as $list) { echo count($list), "\n"; }
"#;
    let (_, output) = crate::common::run_code_capture_output(code).unwrap();
    assert!(output.contains("1\n1\n1\n1\n"));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --test reflection_attributes test_reflection_function_method_property_param_const_attributes`
Expected: FAIL (no attribute attachment or getAttributes support).

**Step 3: Write minimal implementation**

- Add opcodes like `SetClassAttributes`, `SetMethodAttributes`, `SetFunctionAttributes`, `SetPropertyAttributes`, `SetParameterAttributes`, `SetClassConstAttributes`, `SetGlobalConstAttributes` using `u16` constant indices for attribute payloads.
- Emit these opcodes from `Emitter` for class/function/method/property/param/const nodes by lowering AST attribute groups into constant payloads (e.g., `Val::Array` structure: `[{name, args, line, offset}]`).
- In VM, interpret opcodes to deserialize payloads into `AttributeInstance` and attach to runtime metadata.

**Step 4: Run tests to verify they pass**

Run: `cargo test --test reflection_attributes test_reflection_function_method_property_param_const_attributes`
Expected: PASS

**Step 5: Commit**

```bash
git add src/compiler/emitter.rs src/vm/opcode.rs src/vm/engine.rs src/runtime/context.rs tests/reflection_attributes.rs
git commit -m "Attach attributes to runtime metadata"
```

---

### Task 4: Implement target/repeatable validation and `Attribute` class metadata

**Files:**
- Modify: `src/builtins/reflection.rs`
- Modify: `src/runtime/context.rs`
- Modify: `src/vm/engine.rs`
- Modify: `src/runtime/core_extension.rs`
- Test: `tests/reflection_attributes.rs`

**Step 1: Write the failing tests**

```rust
#[test]
fn test_attribute_target_validation_on_new_instance() {
    let code = r#"<?php
#[Attribute(Attribute::TARGET_PROPERTY)]
class OnlyProp {}
#[OnlyProp]
class Foo {}
$attrs = (new ReflectionClass(Foo::class))->getAttributes();
try { $attrs[0]->newInstance(); }
catch (Throwable $e) { echo $e->getMessage(), "\n"; }
"#;
    let (_, output) = crate::common::run_code_capture_output(code).unwrap();
    assert!(output.contains("cannot target"));
}

#[test]
fn test_attribute_repeatable_validation_on_new_instance() {
    let code = r#"<?php
#[Attribute]
class NonRepeat {}
#[NonRepeat]
#[NonRepeat]
class Foo {}
$attrs = (new ReflectionClass(Foo::class))->getAttributes();
try { $attrs[0]->newInstance(); }
catch (Throwable $e) { echo $e->getMessage(), "\n"; }
"#;
    let (_, output) = crate::common::run_code_capture_output(code).unwrap();
    assert!(output.contains("must not be repeated"));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --test reflection_attributes test_attribute_target_validation_on_new_instance`
Expected: FAIL (newInstance returns null/does not validate).

**Step 3: Write minimal implementation**

- Define `Attribute` class in `CoreExtension` with target constants and `IS_REPEATABLE`.
- When a class is declared with `#[Attribute]`, store `AttributeClassInfo { targets, is_repeatable }` in the class metadata.
- Implement validation rules per PHP 8.5 (`Zend/zend_attributes.h`, `zend_compile.c`):
  - Target mask checks.
  - Repeatable checks.
  - Optional delayed validation for internal attributes (stub if not supported).
- `ReflectionAttribute::newInstance()` performs autoload if needed and enforces user attribute target/repeatable rules.

**Step 4: Run tests to verify they pass**

Run: `cargo test --test reflection_attributes test_attribute_target_validation_on_new_instance`
Expected: PASS

**Step 5: Commit**

```bash
git add src/builtins/reflection.rs src/runtime/context.rs src/vm/engine.rs src/runtime/core_extension.rs tests/reflection_attributes.rs
git commit -m "Validate attribute targets and repeatability"
```

---

### Task 5: Reflection filtering + `ReflectionAttribute::newInstance()`

**Files:**
- Modify: `src/builtins/reflection.rs`
- Modify: `src/vm/engine.rs`
- Test: `tests/reflection_attributes.rs`

**Step 1: Write the failing tests**

```rust
#[test]
fn test_reflection_attribute_filters() {
    let code = r#"<?php
#[Attribute]
class Marker {}
#[Marker]
class Foo {}
$ref = new ReflectionClass(Foo::class);
$by_name = $ref->getAttributes('Marker');
$by_instanceof = $ref->getAttributes(Marker::class, ReflectionAttribute::IS_INSTANCEOF);
echo count($by_name), "\n";
echo count($by_instanceof), "\n";
"#;
    let (_, output) = crate::common::run_code_capture_output(code).unwrap();
    assert!(output.contains("1\n1\n"));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --test reflection_attributes test_reflection_attribute_filters`
Expected: FAIL

**Step 3: Write minimal implementation**

- Implement `getAttributes($name = null, $flags = 0)` for ReflectionClass/Function/Method/Property/Parameter/ClassConstant/Const.
- Support `ReflectionAttribute::IS_INSTANCEOF` filtering (resolve class; error if missing).
- Implement name-based filtering (case-insensitive).
- Construct `ReflectionAttribute` objects with `name`, `arguments`, `target`, `isRepeated` properties.
- Implement `ReflectionAttribute::newInstance()` using class lookup + autoload + constructor call.

**Step 4: Run tests to verify they pass**

Run: `cargo test --test reflection_attributes test_reflection_attribute_filters`
Expected: PASS

**Step 5: Commit**

```bash
git add src/builtins/reflection.rs src/vm/engine.rs tests/reflection_attributes.rs
git commit -m "Implement ReflectionAttribute filtering and instantiation"
```

---

### Task 6: End-to-end attribute reflection test coverage

**Files:**
- Modify: `tests/reflection_attributes.rs`

**Step 1: Write the failing tests**

```rust
#[test]
fn test_global_constant_attributes() {
    let code = r#"<?php
#[ExampleConst]
const X = 1;
$ref = new ReflectionConstant('X');
$attrs = $ref->getAttributes();
echo count($attrs), "\n";
"#;
    let (_, output) = crate::common::run_code_capture_output(code).unwrap();
    assert!(output.contains("1\n"));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --test reflection_attributes test_global_constant_attributes`
Expected: FAIL

**Step 3: Write minimal implementation**

- Ensure global constants store attributes metadata.
- Wire `ReflectionConstant::getAttributes()` to metadata.

**Step 4: Run test to verify it passes**

Run: `cargo test --test reflection_attributes test_global_constant_attributes`
Expected: PASS

**Step 5: Commit**

```bash
git add tests/reflection_attributes.rs
git commit -m "Add global constant attribute reflection tests"
```

---

### Task 7: Documentation + plan registry update

**Files:**
- Modify: `docs/IMPLEMENTATION_PLANS.md`

**Step 1: Add plan entry**

```md
- 2026-01-07: Attribute reflection (PHP 8.5) (`docs/plans/2026-01-07-attribute-reflection.md`)
```

**Step 2: Commit**

```bash
git add docs/IMPLEMENTATION_PLANS.md docs/plans/2026-01-07-attribute-reflection.md
git commit -m "Document attribute reflection plan"
```

