# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

php-rs is a PHP interpreter written in Rust, currently in experimental state. It implements core PHP language features, standard library extensions (BCMath, JSON, MySQLi, PDO, OpenSSL, Zip, Zlib), and provides both CLI (`php`) and FastCGI Process Manager (`php-fpm`) interfaces.

### PHP Reference Source

**The official PHP source code is available locally at `$PHP_SRC_PATH`** for reference and study. This is essential for:
- Understanding how PHP implements features before implementing them in Rust
- Verifying correct behavior and edge cases
- Ensuring feature parity and compatibility
- Researching PHP internals and implementation details

Always consult the PHP source code before implementing new features to ensure accurate behavior replication.

## Build, Test, and Development Commands

### Building
- `cargo build` - Compile the interpreter and binaries
- `cargo build --release` - Production build with optimizations

### Running
- `cargo run --bin php -- <script.php>` - Execute a PHP script
- `cargo run --bin php` - Start interactive shell
- `cargo run --bin php-fpm` - Start FPM server

### Testing
- `cargo test` - Run full test suite (unit + integration)
- `cargo test --test array_functions` - Run single integration test file
- `cargo fmt` - Format code with rustfmt

### PHP Reflection Tools
- `php --rf <functionName>` - View function signatures
- `php --rc <className>` - View class declaration information
- `php --re <extensionName>` - View extension information
- `php --ri <extensionName>` - View extension runtime information

## Architecture

### Pipeline: Source → AST → Bytecode → Execution

1. **Lexer** (`src/parser/lexer/`) - Tokenizes PHP source code
2. **Parser** (`src/parser/parser/`) - Produces AST from tokens
3. **AST** (`src/parser/ast/`) - Abstract syntax tree representation
4. **Compiler** (`src/compiler/`) - Compiles AST to bytecode chunks (`chunk.rs`, `emitter.rs`)
5. **VM** (`src/vm/`) - Executes bytecode via `engine.rs` and `executor.rs`

### Core Systems

**Value System** (`src/core/`)
- `value.rs` - PHP value representation in Rust
- `array.rs` - PHP array implementation
- `heap.rs` - Memory management with arena allocation
- `interner.rs` - String interning for performance

**Runtime** (`src/runtime/`)
- `context.rs` - Execution context and state (23KB, central runtime orchestration)
- `registry.rs` - Type and function registration
- `extension.rs` - Extension interface
- `*_extension.rs` - Built-in extensions (core, date, json, hash, openssl, zlib, mysqli, pdo, mb, zip, pthreads)
- `resource_manager.rs` - External resource lifecycle management
- `attributes.rs` - PHP 8 attribute support

**VM Execution** (`src/vm/`)
- `engine.rs` - Main VM engine (595KB, handles instruction execution)
- `executor.rs` - High-level execution coordination (30KB)
- `callable.rs` - Function/method invocation
- `class_resolution.rs` - Class and interface resolution
- `frame.rs` & `frame_helpers.rs` - Call stack management
- `memory.rs` - Runtime memory operations
- `array_access.rs`, `assign_op.rs`, `inc_dec.rs` - Operation implementations

**Built-ins** (`src/builtins/`)
- PHP built-in functions organized by category (array, string, file, etc.)
- Route new built-ins through `src/builtins/mod.rs`

**SAPI** (`src/sapi/`)
- Server API abstraction layer

**FastCGI** (`src/fcgi/`)
- `protocol.rs` - FastCGI protocol implementation
- `request.rs` - Request handling

**Binaries** (`src/bin/`)
- `php.rs` - CLI entry point
- `php-fpm.rs` - FPM entry point
- `dump_bytecode.rs` - Debug tool for bytecode inspection

**PHPT Test Runner** (`src/phpt/`)
- `parser.rs` - .phpt file parser supporting all standard sections (TEST, FILE, EXPECT, EXPECTF, EXPECTREGEX, SKIPIF, INI, ENV, ARGS, CLEAN)
- `executor.rs` - Test execution engine with isolated VM instances
- `matcher.rs` - Output matching with support for exact, format placeholders (%s, %d, %i, %f, %c, %e, %a, %A, %w, %r...%r), and regex
- `output_writer.rs` - Thread-safe buffered output capture
- `results.rs` - Test result tracking and reporting

### Testing Structure

- **Integration tests**: `tests/*.rs` - Feature-level tests (e.g., `array_functions.rs`, `strict_types_eval.rs`)
- **Parser tests**: `src/parser/tests/` with snapshots in `src/parser/tests/snapshots/`
- **PHPT tests**: `cargo run --bin php -- $PHP_SRC_PATH/run-tests.php $PHP_SRC_PATH` to run official PHP .phpt tests for compatibility verification
- Tests should verify PHP compatibility behavior

## Development Workflow

### Before Implementation
1. Study PHP source code (available at `$PHP_SRC_PATH` environment variable)
2. Check for existing .phpt tests at `$PHP_SRC_PATH/tests/` that cover the feature
3. Fully understand the PHP feature to be implemented
4. Plan all related modules and data structures (lexer, parser, AST, bytecode, VM instructions, runtime)
5. Write implementation plan to `docs/IMPLEMENTATION_PLANS.md`
6. Discuss plan with team for feedback

### Implementation
1. Implement feature in Rust following project standards
2. Add tests covering edge cases (both Rust integration tests and .phpt tests if available)
3. Run full test suite to ensure no regressions (`cargo test`)
4. Run relevant .phpt tests from PHP source if applicable (`cargo run --bin php -- $PHP_SRC_PATH/run-tests.php $PHP_SRC_PATH`)
5. Remove temporary code, docs, or debug statements

### Key Principles
- Follow standard Rust formatting with default rustfmt settings
- Use `snake_case` for functions/modules, `CamelCase` for types, `SCREAMING_SNAKE_CASE` for constants
- Keep modules focused; add new built-ins in `src/builtins/` and route through `src/builtins/mod.rs`
- Boldly refactor without backward compatibility concerns, as long as PHP behavior remains unchanged
- Do not implement features that don't exist in PHP
- Prioritize readability and correctness over premature optimization
- Leverage Rust's type system, ownership model, and pattern matching
- Use `Result` and `Option` types appropriately for error handling
- Document public APIs with Rustdoc comments

### Extension Dependencies
Extension-related crates are scoped to specific modules. New dependencies should be justified and kept scoped to the relevant extension:
- Hash: md-5, sha1, sha2, sha3, whirlpool, etc.
- MySQLi: mysql crate
- PDO: rusqlite, postgres, oracle
- Zlib: flate2
- Zip: zip
- OpenSSL: openssl, base64
- BCMath: num-bigint, rust_decimal

## Important Notes

- No explicit test coverage target; keep tests representative of PHP compatibility
- Use .phpt tests from `$PHP_SRC_PATH/tests/` to validate compatibility with official PHP
- Commit messages should use short, imperative subjects (e.g., "Add array unpack tests")
- Reasons required for incomplete/partial completion; no summary needed for complete work
- Parser behavior changes should include or update snapshot files
