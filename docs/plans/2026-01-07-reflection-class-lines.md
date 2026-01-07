# ReflectionClass line tracking implementation plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement PHP-compatible `ReflectionClass::getStartLine()`/`getEndLine()` using stored class line metadata.

**Architecture:** Capture class/interface/trait/enum closing brace spans in the parser, compute 1-based line numbers in the emitter using source bytes, and persist them in `ClassDef` via a new opcode. Reflection returns the stored values or `false` for internal/unknown classes.

**Tech Stack:** Rust, parser AST, bytecode emitter, VM opcodes, builtins reflection.

### Task 1: Add failing reflection test

**Files:**
- Modify: `tests/reflection_test.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn test_reflection_class_get_start_end_line() {
    let result = run_php(r#"<?php
class TestClass
{
    public function foo() {}
}
$rc = new ReflectionClass('TestClass');
return $rc->getStartLine() . ',' . $rc->getEndLine();
"#);

    assert_eq!(result, Val::String(Rc::new(b"2,5".to_vec())));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --test reflection_test test_reflection_class_get_start_end_line`
Expected: FAIL because both methods currently return `false`.

**Step 3: Commit**

```bash
git add tests/reflection_test.rs
git commit -m "Add failing test for ReflectionClass line numbers"
```

### Task 2: Capture closing brace span in parser AST

**Files:**
- Modify: `src/parser/ast/mod.rs`
- Modify: `src/parser/parser/definitions.rs`

**Step 1: Add line span fields to class-like statements**

Add optional brace span (or explicit line fields) to `Stmt::Class`, `Stmt::Interface`, `Stmt::Trait`, and `Stmt::Enum`:

```rust
Stmt::Class {
    // ...existing fields...
    close_brace_span: Option<Span>,
    span: Span,
}
```

**Step 2: Record the closing brace span before bump**

```rust
let close_brace_span = if self.current_token.kind == TokenKind::CloseBrace {
    Some(self.current_token.span)
} else {
    None
};
if self.current_token.kind == TokenKind::CloseBrace {
    self.bump();
} else {
    self.errors.push(ParseError { /* ... */ });
}
let end = close_brace_span.map(|s| s.end).unwrap_or(self.current_token.span.end);
```

Store `close_brace_span` on the statement. Apply this pattern in `parse_class`, `parse_interface`, `parse_trait`, and `parse_enum`.

**Step 3: Commit**

```bash
git add src/parser/ast/mod.rs src/parser/parser/definitions.rs
git commit -m "Track closing brace span for class-like statements"
```

### Task 3: Emit line metadata into ClassDef

**Files:**
- Modify: `src/compiler/emitter.rs`
- Modify: `src/vm/opcode.rs`
- Modify: `src/vm/engine.rs`
- Modify: `src/runtime/context.rs`

**Step 1: Add line fields to ClassDef**

```rust
pub struct ClassDef {
    // ...
    pub start_line: Option<u32>,
    pub end_line: Option<u32>,
}
```

Initialize both fields to `None` for internal classes and defaults.

**Step 2: Add opcode to carry line info**

```rust
pub enum OpCode {
    // ...
    SetClassLines(Symbol, Option<u32>, Option<u32>),
}
```

**Step 3: Emit line metadata in class/interface/trait/enum paths**

Compute line numbers using spans and the emitter source:

```rust
let start_line = span.line_info(self.source).map(|info| info.line as u32);
let end_line = close_brace_span.and_then(|s| s.line_info(self.source).map(|info| info.line as u32));
self.chunk.code.push(OpCode::SetClassLines(class_sym, start_line, end_line));
```

**Step 4: Handle opcode in VM**

```rust
OpCode::SetClassLines(class_sym, start_line, end_line) => {
    if let Some(class_def) = self.context.get_class_mut(class_sym) {
        class_def.start_line = start_line;
        class_def.end_line = end_line;
    }
}
```

**Step 5: Commit**

```bash
git add src/compiler/emitter.rs src/vm/opcode.rs src/vm/engine.rs src/runtime/context.rs
git commit -m "Track class line numbers in ClassDef"
```

### Task 4: Implement ReflectionClass start/end line methods

**Files:**
- Modify: `src/builtins/reflection.rs`

**Step 1: Return stored values or false**

```rust
if class_def.is_internal {
    return Ok(vm.arena.alloc(Val::Bool(false)));
}
if let Some(start_line) = class_def.start_line {
    return Ok(vm.arena.alloc(Val::Int(start_line as i64)));
}
Ok(vm.arena.alloc(Val::Bool(false)))
```

Mirror this for `end_line`.

**Step 2: Commit**

```bash
git add src/builtins/reflection.rs
git commit -m "Implement ReflectionClass start/end line accessors"
```

### Task 5: Verify

**Files:**
- Test: `tests/reflection_test.rs`

**Step 1: Run targeted test**

Run: `cargo test --test reflection_test test_reflection_class_get_start_end_line`
Expected: PASS

**Step 2: Run full test suite (optional if time)**

Run: `cargo test`
Expected: PASS (existing warnings ok)

**Step 3: Commit verification (if needed)**

No code changes expected; no commit.
