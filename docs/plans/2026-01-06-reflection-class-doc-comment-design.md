# Reflection Class Doc Comment Design

**Goal:** Store and expose class-like doc comments (class/interface/trait) through ReflectionClass::getDocComment().

**Architecture:**
- Add a doc comment field to `ClassDef` and populate it at class definition time.
- The emitter reads the doc comment span from the AST and emits a dedicated opcode with a string constant payload.
- The VM handles the opcode by setting the class metadata, and reflection returns the stored value or `false`.

**Scope:**
- Applies to class, interface, and trait declarations.
- Enum handling is deferred until enums are emitted in the compiler/VM.

**Data Flow:**
1. Parser attaches `doc_comment: Option<Span>` to class-like AST nodes (already present).
2. Emitter slices source bytes from the span, stores as a string constant, emits `SetClassDocComment` opcode.
3. VM applies the opcode to the `ClassDef` for the target class symbol.
4. Reflection reads `ClassDef.doc_comment` and returns the string or `false`.

**Error Handling:**
- Span bounds issues are treated as compiler bugs; no new runtime errors introduced.
- Missing doc comments return `false` (matches PHP behavior).

**Testing:**
- Integration test to assert ReflectionClass::getDocComment() returns the exact doc block for class/interface/trait.
- Test a class without a doc comment returning `false`.
- Optional: doc comment placed above attributes to confirm capture is unaffected.
