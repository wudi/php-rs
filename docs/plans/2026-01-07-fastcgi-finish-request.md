# Implementation Plan - fastcgi_finish_request

## Goal
Implement `fastcgi_finish_request()` function for PHP-FPM SAPI. This function flushes all open output buffers to the client, sends the response headers and content, and finishes the FastCGI request, allowing the script to continue running in the background.

## Changes

### 1. VM Engine (`src/vm/engine.rs`)
- Updated `OutputWriter` trait to include `send_headers` and `finish` methods.
- Added `finish_request` method to `VM` struct which coordinates header sending, flushing, and finishing via the output writer.

### 2. Builtins (`src/builtins/fastcgi.rs`)
- Created new module `fastcgi`.
- Implemented `fastcgi_finish_request` function which calls `vm.finish_request()`.

### 3. Registration (`src/runtime/core_extension.rs`)
- Registered `fastcgi_finish_request` in `CoreExtension`.

### 4. PHP-FPM Binary (`src/bin/php-fpm.rs`)
- Replaced `BufferedOutputWriter` (which buffered entire response) with `FpmOutputWriter`.
- `FpmOutputWriter` supports:
    - Buffering output (default).
    - `send_headers`: Sends FastCGI STDOUT record with headers.
    - `flush`: Flushes buffer to FastCGI stream (only if headers sent).
    - `finish`: Sends remaining buffer, empty STDOUT, and END_REQUEST record. Marks request as finished.
- Updated `execute_php` to use the new writer and handle cases where the script finishes early (via `fastcgi_finish_request`) vs normally.

### 5. Testing
- Added integration test `test_fpm_finish_request` in `tests/fpm_integration_test.rs` verifying that output after `fastcgi_finish_request` is not sent to client but script continues execution.

## Verification
- `cargo check` passes.
- `cargo test --test fpm_integration_test` passes.
