# Repository Guidelines

## Project Structure & Module Organization
- `src/` contains the Rust implementation of the PHP interpreter. Major areas include the parser (`src/parser/`), VM/executor (`src/vm/`), runtime extensions (`src/runtime/`), and builtins (`src/builtins/`).
- `src/bin/` hosts the CLI entry points: `php` and `php-fpm`.
- `tests/` holds integration tests organized by feature (for example, `tests/array_functions.rs`).
- Parser-specific tests live under `src/parser/tests/`, with snapshot fixtures in `src/parser/tests/snapshots/`.

## Build, Test, and Development Commands
- `cargo build`: compile the interpreter and binaries.
- `cargo test`: run the full Rust test suite (unit + integration).
- `cargo test --test array_functions`: run a single integration test file.
- `cargo run --bin php -- <script.php>`: execute a PHP script with the CLI binary.
- `cargo run --bin php-fpm`: start the FPM server implementation.

## Coding Style & Naming Conventions
- Follow standard Rust formatting (`cargo fmt`) with default rustfmt settings.
- Use `snake_case` for functions/modules, `CamelCase` for types, and `SCREAMING_SNAKE_CASE` for constants.
- Keep modules focused; prefer adding new builtins in `src/builtins/` and routing them through `src/builtins/mod.rs`.

## Testing Guidelines
- Add new integration tests under `tests/` using descriptive file names (for example, `tests/strict_types_eval.rs`).
- Parser behavior changes should include or update snapshot files under `src/parser/tests/snapshots/`.
- There is no explicit coverage target; keep tests representative of PHP compatibility behavior.

## Commit & Pull Request Guidelines
- Commit history is minimal (only an init commit), so no established convention exists. Use short, imperative subjects (for example, "Add array unpack tests").
- PRs should include: a brief description of behavior changes, tests run (`cargo test` or targeted tests), and any relevant PHP compatibility notes.

## Configuration & Dependencies Notes
- The project depends on several extension-related crates (OpenSSL, MySQL, PDO drivers). Keep new dependencies justified and scoped to the relevant extension module.

## Tools for view info:
- View function signatures via `php --rf <functionName>`
- View class declaration information via `php --rc <className>`
- Test native php result via `php` command
- Reflection Example from Shell `php --rf strlen` `php --rc finfo` `php --re json` `php --ri dom`

## Programming Style
- Follow Rust idioms and best practices.
- Write clear, maintainable code with comments explaining complex logic.
- Avoid unnecessary optimizations; prioritize readability and correctness.
- Modularize code effectively to enhance maintainability and scalability.
- Adhere to SOLID principles where applicable.
- Use error handling best practices, leveraging Rust's `Result` and `Option` types appropriately.
- Document public APIs with Rustdoc comments for clarity and usability.
- Utilize Rust's powerful type system to enforce invariants and reduce runtime errors.
- Leverage Rust's concurrency features safely when dealing with multi-threaded contexts.
- Employ Rust's pattern matching capabilities to simplify control flow and enhance code clarity.
- Make use of Rust's standard library and ecosystem crates to avoid reinventing the wheel.
- Write unit tests for critical components to ensure reliability and facilitate future changes.
- Engage in code reviews to maintain code quality and share knowledge among team members.
- Continuously refactor code to improve structure and eliminate technical debt.
- Boldly refactor without backward compatibility, as long as PHP behavior remains unchanged.
- Strive for idiomatic Rust code that is easy to understand and maintain.
- Use Rust's module system effectively to organize code and manage visibility.
- Follow Rust's naming conventions consistently throughout the codebase.
- Utilize Rust's macros judiciously to reduce boilerplate while maintaining code clarity.
- Embrace Rust's ownership model to ensure memory safety

## Requirements
- Rust toolchain (stable) installed via [rustup](https://rustup.rs/).
- PHP source code available in the local folder specified by the environment variable `$PHP_SRC_PATH` for reference and study.
- Familiarity with PHP internals and behavior to ensure feature parity.
- Before implementing each feature, it is necessary to research how PHP implements it, and do not implement features that do not exist in PHP.

## Workflow
- Fully understand the PHP feature to be implemented by studying the PHP source code.
- All related modules and data structures should be planned in advance. e.g., lexer, parser, AST, SExpr, VM instructions, runtime structures, etc.
- Planning the implementation approach, considering how to map PHP concepts to Rust constructs.
- Write plans in to docs/IMPLEMENTATION_PLANS.md for tracking and discussion.
- Discuss the plan with the team to gather feedback and suggestions.
- Implement the feature in Rust, adhering to the project's coding standards and guidelines.
- Write tests to verify the correctness of the implementation, ensuring they cover various edge cases.
- Run the full test suite to ensure no existing functionality is broken.
- Remove any temporary code, docs or debug statements used during development.
- No summary is required after each completion, but reasons are required for incomplete/partial completion.