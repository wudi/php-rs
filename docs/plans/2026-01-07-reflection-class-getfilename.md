# ReflectionClass::getFileName Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Track class source filenames so `ReflectionClass::getFileName()` returns the defining file for user classes and `false` for internal classes.

**Architecture:** Store an optional file path on `ClassDef` sourced from the executing `CodeChunk.file_path`. During class definition opcodes, capture the current frame’s file path (canonical absolute) and persist it on the class metadata. Reflection reads this field and mirrors PHP behavior.

**Tech Stack:** Rust, VM runtime (`src/vm/engine.rs`), compiler emitter (`src/compiler/emitter.rs`), reflection builtins.

### Task 1: Add failing test for class file name

**Files:**
- Modify: `tests/reflection_test.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn test_reflection_class_get_file_name() {
    use php_rs::compiler::emitter::Emitter;
    use php_rs::runtime::context::{EngineBuilder, RequestContext};
    use php_rs::vm::engine::VM;
    use std::rc::Rc;
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("TestClass.php");
    let file_path_str = file_path.to_string_lossy().into_owned();

    let source = r#"<?php
        class TestClass {}
        $rc = new ReflectionClass('TestClass');
        return $rc->getFileName();
    "#;

    let arena = bumpalo::Bump::new();
    let lexer = php_rs::parser::lexer::Lexer::new(source.as_bytes());
    let mut parser = php_rs::parser::parser::Parser::new(lexer, &arena);
    let program = parser.parse_program();
    assert!(program.errors.is_empty());

    let context = EngineBuilder::new()
        .with_core_extensions()
        .build()
        .expect("Failed to build engine");
    let mut request_context = RequestContext::new(context);
    let emitter = Emitter::new(source.as_bytes(), &mut request_context.interner)
        .with_file_path(file_path_str.clone());
    let (chunk, _) = emitter.compile(&program.statements);

    let mut vm = VM::new_with_context(request_context);
    vm.run(Rc::new(chunk)).expect("execution failed");

    let value = match vm.last_return_value {
        Some(handle) => vm.arena.get(handle).value.clone(),
        None => php_rs::core::value::Val::Null,
    };

    assert_eq!(value, php_rs::core::value::Val::String(Rc::new(file_path_str.into_bytes())));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --test reflection_test`
Expected: FAIL because `getFileName()` still returns `false`.

**Step 3: Commit**

```bash
git add tests/reflection_test.rs
git commit -m "Add failing test for ReflectionClass::getFileName"
```

### Task 2: Track class file paths in metadata

**Files:**
- Modify: `src/runtime/context.rs`
- Modify: `src/vm/engine.rs`
- Modify: `src/vm/class_resolution.rs`
- Modify: `src/runtime/context.rs`
- Modify: `src/builtins/reflection.rs`

**Step 1: Update ClassDef with file metadata**

```rust
pub struct ClassDef {
    // ...
    pub doc_comment: Option<Rc<Vec<u8>>>,
    pub file_name: Option<Rc<Vec<u8>>>,
    pub is_internal: bool,
}
```

**Step 2: Populate file_name for user-defined classes**

```rust
let file_name = self
    .frames
    .last()
    .and_then(|frame| frame.chunk.file_path.as_ref())
    .map(|path| Rc::new(path.as_bytes().to_vec()));

let class_def = ClassDef {
    // ...
    file_name,
    is_internal: false,
};
```

Apply to `OpCode::DeclareClass`, `OpCode::DefClass`, `OpCode::DefInterface`, and `OpCode::DefTrait`. Set `file_name: None` for internal/native classes and for test-only class defs in `src/vm/class_resolution.rs`.

**Step 3: Update ReflectionClass::getFileName**

```rust
if class_def.is_internal {
    return Ok(vm.arena.alloc(Val::Bool(false)));
}
if let Some(file_name) = &class_def.file_name {
    return Ok(vm.arena.alloc(Val::String(file_name.clone())));
}
Ok(vm.arena.alloc(Val::Bool(false)))
```

**Step 4: Run tests**

Run: `cargo test --test reflection_test`
Expected: PASS.

**Step 5: Commit**

```bash
git add src/runtime/context.rs src/vm/engine.rs src/vm/class_resolution.rs src/builtins/reflection.rs
git commit -m "Track class file names for reflection"
```

### Task 3: Update implementation plan index

**Files:**
- Modify: `docs/IMPLEMENTATION_PLANS.md`

**Step 1: Add plan entry**

```markdown
- 2026-01-07 ReflectionClass::getFileName (docs/plans/2026-01-07-reflection-class-getfilename.md)
```

**Step 2: Commit**

```bash
git add docs/IMPLEMENTATION_PLANS.md
git commit -m "Document ReflectionClass::getFileName plan"
```
