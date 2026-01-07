# ReflectionClass::getFileName Design

## Overview

This change adds class file name tracking to the runtime metadata so `ReflectionClass::getFileName()` mirrors PHP: user-defined classes return the absolute defining file path and internal classes return `false`. The runtime already carries the canonical path on `CodeChunk.file_path` whenever a file-based script is compiled (CLI entrypoint, include/require). We will reuse that source of truth and persist it on `ClassDef` at the moment a class, interface, or trait is defined. This keeps reflection fast and avoids later inference. For eval() or scripts compiled without a file path, the field will remain unset, and reflection will return `false`, matching the behavior for non-file-backed definitions.

## Architecture and Data Flow

`ClassDef` will gain a new optional field `file_name: Option<Rc<Vec<u8>>>` alongside `doc_comment` and `is_internal`. During opcode execution (`OpCode::DeclareClass`, `OpCode::DefClass`, `OpCode::DefInterface`, `OpCode::DefTrait`), the VM will read the current frame’s `chunk.file_path`, convert it to raw bytes, and attach it to the newly created `ClassDef`. For internal/native classes materialized from extensions, `file_name` remains `None`. `ReflectionClass::getFileName()` will then check `is_internal` first; if true, return `false`. Otherwise, return the stored filename if present, or `false` if absent. This mirrors PHP’s logic and is stable for later reflection calls even after includes or nested executions.

## Error Handling and Edge Cases

No new error conditions are introduced. If a class is defined in eval() or compiled without a file path, the stored filename is `None` and reflection returns `false`. If a class is internal, reflection returns `false` regardless of any stored filename. This ensures that errors remain unchanged and avoids coupling file tracking to any IO or path resolution. The change does not affect class loading, inheritance resolution, or runtime instantiation.

## Testing Strategy

Add a reflection test that compiles a class with a provided file path via the emitter and asserts that `ReflectionClass::getFileName()` returns the exact byte string. Use a temporary directory to generate a stable absolute path and avoid reliance on repository layout. The test should fail before the implementation because reflection currently returns `false`, and pass after the metadata is stored and read back. Additional tests for internal classes are already covered by existing reflection tests, so no new coverage is required there.
