# PHPT Basic Tests - Implementation Plan

**Test Suite**: `/Users/eagle/Sourcecode/php-src/tests/basic`
**Current Status**: 10/110 passing (9% pass rate)
**Last Updated**: 2026-01-26

## Overview

This document tracks the implementation plan for fixing all 100 failing tests in the PHP basic test suite. Each issue is categorized and prioritized by impact.

## Critical Issues (High Impact - Fixes 50+ tests)

### 1. Superglobals Support ⚠️ IN PROGRESS
**Impact**: ~35 tests
**Status**: Parser complete, executor needs fixing
**Priority**: CRITICAL

#### Implementation Tasks:
- [x] Add POST/GET/COOKIE sections to PHPT parser
- [ ] Fix type system usage in executor (Symbol vs String, Handle vs Value)
- [ ] Implement proper superglobal initialization in VM context
- [ ] Populate $_POST from --POST-- section
- [ ] Populate $_GET from --GET-- section
- [ ] Populate $_COOKIE from --COOKIE-- section
- [ ] Populate $_REQUEST (merged POST/GET/COOKIE)
- [ ] Populate $_SERVER with basic SAPI information
- [ ] Add URL decoding for query string parsing
- [ ] Test with basic/002.phpt through basic/005.phpt

#### Files to Modify:
- `src/phpt/executor.rs` - Fix type system usage
- `src/runtime/context.rs` - Ensure superglobals are pre-registered
- `src/vm/engine.rs` - Initialize superglobals on VM startup

#### Reference Implementation:
Check PHP source: `$PHP_SRC_PATH/main/php_variables.c`

### 2. Missing Built-in Functions ❌ NOT STARTED
**Impact**: ~40 tests
**Status**: Not implemented
**Priority**: CRITICAL

#### Required Functions by Category:

##### SAPI Functions
- [ ] `php_sapi_name()` - Returns "cli", "fpm", etc.
- [ ] `php_uname()` - System information

##### INI Functions
- [ ] `ini_get(string $option): string|false`
- [ ] `ini_set(string $option, string $value): string|false`
- [ ] `ini_restore(string $option): void`
- [ ] `ini_get_all(?string $extension = null, bool $details = true): array`
- [ ] `get_cfg_var(string $option): string|false`

##### Execution Control
- [ ] `set_time_limit(int $seconds): bool`
- [ ] `ignore_user_abort(bool $value = ?): int`
- [ ] `connection_aborted(): int`
- [ ] `connection_status(): int`

##### Process Functions
- [ ] `getmypid(): int|false`
- [ ] `getmyuid(): int|false`
- [ ] `getmygid(): int|false`
- [ ] `getmyinode(): int|false`
- [ ] `getlastmod(): int|false`

##### File Operations (Missing)
- [ ] `rename(string $from, string $to, ?resource $context = null): bool`
- [ ] `touch(string $filename, ?int $mtime = null, ?int $atime = null): bool`
- [ ] `chmod(string $filename, int $permissions): bool`
- [ ] `chown(string $filename, string|int $user): bool`
- [ ] `chgrp(string $filename, string|int $group): bool`

##### Error Handling
- [ ] `error_get_last(): ?array`
- [ ] `error_clear_last(): void`
- [ ] `restore_error_handler(): bool`
- [ ] `restore_exception_handler(): bool`

##### Output Control
- [ ] `ob_list_handlers(): array`
- [ ] `ob_get_status(bool $full = false): array`
- [ ] `flush(): void` - Flush system output buffer

##### Variable Functions
- [ ] `get_defined_vars(): array`
- [ ] `import_request_variables()` - Deprecated but tested
- [ ] `extract(array $array, int $flags = EXTR_OVERWRITE, string $prefix = ""): int`

#### Implementation Approach:
1. Create new built-in modules as needed
2. Add functions to `src/builtins/` with appropriate module
3. Register in `src/runtime/extension.rs`
4. Test each function with relevant .phpt tests

#### Files to Create/Modify:
- `src/builtins/sapi.rs` - New file for SAPI functions
- `src/builtins/ini.rs` - New file for INI functions
- `src/builtins/execution.rs` - New file for execution control
- `src/builtins/process.rs` - New file for process functions
- `src/builtins/file.rs` - Extend existing file operations
- `src/builtins/mod.rs` - Register new modules

### 3. INI System Implementation ❌ NOT STARTED
**Impact**: ~25 tests
**Status**: Not implemented
**Priority**: HIGH

#### Requirements:
- [ ] Global INI registry in runtime context
- [ ] Default INI values (error_reporting, display_errors, etc.)
- [ ] INI parsing from --INI-- section in PHPT tests
- [ ] Runtime INI modification (ini_set)
- [ ] INI restoration (ini_restore)
- [ ] INI scoping (per-directory .htaccess style)

#### Implementation:
```rust
// src/runtime/ini.rs
pub struct IniRegistry {
    values: HashMap<Symbol, IniValue>,
    defaults: HashMap<Symbol, IniValue>,
}

pub enum IniValue {
    String(String),
    Int(i64),
    Bool(bool),
    Float(f64),
}
```

#### Files to Create:
- `src/runtime/ini.rs` - INI system implementation
- `src/runtime/ini_defaults.rs` - Default INI values

#### Reference:
- `$PHP_SRC_PATH/main/php_ini.c`
- `$PHP_SRC_PATH/main/php_ini.h`

## Medium Impact Issues (Fixes 10-20 tests)

### 4. PHPT Parser Edge Cases ❌ NOT STARTED
**Impact**: ~15 tests showing parse errors
**Status**: Not implemented
**Priority**: MEDIUM

#### Issues:
- [ ] Handle files with missing newlines at EOF
- [ ] Handle Windows line endings (CRLF)
- [ ] Handle empty sections
- [ ] Handle malformed section markers
- [ ] Better error messages for invalid .phpt files

#### Failing Tests with Parse Errors:
- basic/021.phpt - "stream did not contain valid UTF-8"
- basic/028.phpt - "stream did not contain valid UTF-8"
- basic/bug67988.phpt - Parse error
- Several others

#### Files to Modify:
- `src/phpt/parser.rs` - Improve error handling

### 5. SKIPIF Function Availability ❌ NOT STARTED
**Impact**: ~37 tests showing SKIPIF errors
**Status**: Missing functions in SKIPIF checks
**Priority**: MEDIUM

Many tests fail because SKIPIF sections call undefined functions:
- `extension_loaded()` - Check if extension is loaded
- `function_exists()` - Check if function exists
- `class_exists()` - Check if class exists

These need to be implemented to allow proper test skipping.

#### Files to Modify:
- `src/builtins/info.rs` - Add reflection functions

### 6. Command-line Arguments (ARGS) ❌ NOT STARTED
**Impact**: ~10 tests
**Status**: Not implemented
**Priority**: MEDIUM

#### Requirements:
- [ ] Parse --ARGS-- section from PHPT
- [ ] Populate $argv array
- [ ] Set $argc variable
- [ ] Handle register_argc_argv INI setting

#### Files to Modify:
- `src/phpt/executor.rs` - Parse ARGS section
- `src/sapi/cli.rs` - Handle argc/argv initialization

## Low Impact Issues (Fixes <10 tests)

### 7. Special SAPI Features ❌ NOT STARTED
**Impact**: ~5 tests
**Status**: Not implemented
**Priority**: LOW

- [ ] php-cgi SAPI (currently skipped)
- [ ] Apache SAPI features
- [ ] HTTP header manipulation
- [ ] FastCGI-specific features

### 8. Windows-specific Tests ⊘ SKIP
**Impact**: 2-3 tests
**Status**: Not applicable on macOS/Linux
**Priority**: SKIP

Tests like `011_windows.phpt` are Windows-specific and can be skipped on other platforms.

## Test Failure Categories

### Categorized by Root Cause:

1. **Missing Superglobals** (35 tests):
   - 002.phpt - $_POST
   - 003.phpt - $_GET + $_POST
   - 004.phpt - $_POST
   - 005.phpt - $_COOKIE
   - 011.phpt - $_SERVER
   - 012.phpt - $argv/$argc
   - Many more...

2. **Missing Built-in Functions** (40 tests):
   - Tests calling php_sapi_name()
   - Tests calling ini_get/ini_set
   - Tests calling set_time_limit()
   - Tests calling getmypid()
   - Tests calling rename()

3. **Parser Errors** (15 tests):
   - 021.phpt - UTF-8 handling
   - 028.phpt - UTF-8 handling
   - bug67988.phpt - Malformed test file

4. **SKIPIF Failures** (37 tests):
   - All timeout_variation_*.phpt tests
   - Tests requiring specific extensions
   - Tests checking function_exists()

5. **Missing Features** (10 tests):
   - ARGS section support
   - Class autoloading
   - Specific PHP 8 features

## Implementation Priority Order

### Phase 1: Core Infrastructure (Week 1)
1. ✅ Fix superglobals type system issues
2. ✅ Implement basic superglobal population
3. ✅ Test POST/GET/COOKIE with basic/002-005.phpt
4. ✅ Implement INI registry system
5. ✅ Add ini_get/ini_set functions

**Expected**: 35 additional tests passing (~45 total)

### Phase 2: Built-in Functions (Week 2)
1. ✅ Implement php_sapi_name() and SAPI functions
2. ✅ Implement process functions (getmypid, etc.)
3. ✅ Implement execution control (set_time_limit)
4. ✅ Add missing file operations (rename, touch, chmod)
5. ✅ Add reflection functions (function_exists, etc.)

**Expected**: 40 additional tests passing (~85 total)

### Phase 3: Edge Cases & Polish (Week 3)
1. ✅ Fix PHPT parser edge cases
2. ✅ Implement ARGS section support
3. ✅ Fix SKIPIF function availability
4. ✅ Handle special test cases

**Expected**: 20 additional tests passing (~105 total)

### Phase 4: Advanced Features (Week 4)
1. ✅ Advanced SAPI features
2. ✅ Windows compatibility (if needed)
3. ✅ Performance optimization
4. ✅ Documentation updates

**Expected**: Remaining tests passing (110/110 = 100%)

## Testing Strategy

### Incremental Testing:
```bash
# Test specific feature
cargo run --bin php-test -- /Users/eagle/Sourcecode/php-src/tests/basic/002.phpt -v

# Test all basic tests
cargo run --bin php-test -- /Users/eagle/Sourcecode/php-src/tests/basic -s

# Test with verbose output
cargo run --bin php-test -- /Users/eagle/Sourcecode/php-src/tests/basic -s -v
```

### Validation:
- Run full test suite after each phase
- Track pass rate improvements
- Document any intentional skips
- Ensure no regressions in passing tests

## Reference Resources

### PHP Source Code:
- Main SAPI: `$PHP_SRC_PATH/main/`
- Built-ins: `$PHP_SRC_PATH/ext/standard/`
- Tests: `$PHP_SRC_PATH/tests/`

### Key Files:
- `$PHP_SRC_PATH/main/php_variables.c` - Superglobal initialization
- `$PHP_SRC_PATH/main/php_ini.c` - INI system
- `$PHP_SRC_PATH/main/SAPI.c` - SAPI interface
- `$PHP_SRC_PATH/ext/standard/info.c` - phpinfo and reflection functions

## Success Criteria

- [ ] 100/110 tests passing (90%+ pass rate)
- [ ] All critical PHP features working
- [ ] Comprehensive test coverage
- [ ] No regressions in existing functionality
- [ ] Clean, maintainable code following Rust best practices

## Notes

- Some tests may be intentionally skipped (Windows-specific, CGI-specific)
- Target is 100+ passing tests, not necessarily 110/110
- Each implementation should consult PHP source code for correct behavior
- All changes should maintain backward compatibility with existing php-rs code
