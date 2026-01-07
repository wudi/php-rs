# ReflectionClass start/end line tracking design

## Goal
Implement PHP-compatible `ReflectionClass::getStartLine()` and
`ReflectionClass::getEndLine()` for user-defined classes, interfaces,
traits, and enums. Internal classes return `false`. Line numbers are
1-based and refer to the class declaration line and the closing brace line.

## Scope
- Add optional line metadata to `ClassDef`.
- Capture accurate start/end line numbers during parsing/compilation.
- Emit metadata into the VM during class declaration.
- Update reflection builtins to return the tracked values.

## Approach
Prefer parse-time line computation, so reflection can return lines without
reparsing source files. Use statement spans to compute 1-based line numbers
against the source bytes already available to the parser/emitter. Track
both start and end lines to match PHP. If a closing brace is missing and the
parser recovers, leave `end_line` as `None`.

## Data model
Extend `ClassDef` with:
- `start_line: Option<u32>`
- `end_line: Option<u32>`

Internal classes keep both fields as `None`.

## Parser changes
`parse_class`, `parse_interface`, `parse_trait`, `parse_enum` should capture
both start and end spans reliably. In particular, record the closing brace
span before `bump()` so the end line refers to the brace itself. Store these
lines on the AST statement (or store the brace span) so the emitter can
consume them.

## Compiler and VM
- Add an opcode such as `OpCode::SetClassLines(class_sym, start, end)`.
- Emit it after `DefClass`/`DefInterface`/`DefTrait`/`DefEnum` if line data is
  present.
- Handle it in the VM by updating `ClassDef`.

## Reflection behavior
`ReflectionClass::getStartLine()` and `getEndLine()` should return the
tracked integers for user-defined classes, or `false` when unavailable or
internal.

## Testing
Add an integration test in `tests/reflection_test.rs` that defines a
multi-line class with a known start and end line, then assert both
reflection values. Keep it focused on classes initially; add interface/trait
coverage only if low-cost.

## Risks
- Off-by-one errors when computing the closing brace line.
- Parser recovery may yield missing `end_line`; ensure `false` is returned.
