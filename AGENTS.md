# Repository Guidelines

## Consistency Guarantee
- Features must maintain the same behavior as native PHP.
- PHP source code in local folder `$PHP_SRC_PATH` for in-depth study of how native PHP is implemented.
- Before implementing each feature, it is necessary to research how PHP implements it.
- Do not implement features that do not exist in PHP.

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

