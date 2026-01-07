# Benchmark Suite Expansion Plan

## Objective
Expand the existing Docker Compose benchmark suite to include test cases of varying complexities. This will allow for a more granular comparison between the native PHP-FPM and the Rust implementation (`php-rs`).

## Current State
- **Infrastructure**: Docker Compose with `php-native` and `php-rust` services + Nginx.
- **Tools**: `wrk` for load generation.
- **Existing Tests**:
  - `hello.php`: Minimal overhead.
  - `fib.php`: Recursive CPU stress.
  - `json.php`: Memory allocation and serialization.

## New Test Cases
We will add three new test scripts to cover different performance aspects:

### 1. `mandelbrot.php` (Iterative Math & Loops)
- **Focus**: Iterative CPU tasks, floating point arithmetic, nested loops.
- **Difference from `fib`**: Iterative vs Recursive. Tests loop performance and math operations.
- **Implementation**: Calculate Mandelbrot set for a fixed grid and output as simple text (or just return count).

### 2. `objects.php` (VM Object Overhead)
- **Focus**: Object instantiation, property access, method dispatch.
- **Logic**:
  - Define a class `User` with private properties and getters/setters.
  - Loop 100,000 times creating instances and modifying them.
  - Tests the efficiency of the object system and memory management (GC).

### 3. `strings.php` (String Manipulation)
- **Focus**: String copying, replacement, searching, and concatenation.
- **Logic**:
  - Take a large base string.
  - Perform `str_replace`, `substr`, `strpos` in a loop.
  - Tests the string implementation efficiency.

## Implementation Steps

1.  **Create PHP Scripts**:
    - Create `benchmarks/src/mandelbrot.php`
    - Create `benchmarks/src/objects.php`
    - Create `benchmarks/src/strings.php`

2.  **Update Runner**:
    - Modify `benchmarks/run_bench.sh` to include execution blocks for the new scripts.

3.  **Execution**:
    - User runs `./benchmarks/run_bench.sh`.

## Verification
- Ensure scripts run without error in standard PHP (using `php-native`).
- Ensure `php-rs` supports all used features (Classes, Math functions, String functions).
