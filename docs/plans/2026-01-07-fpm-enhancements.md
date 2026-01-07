# Implementation Plan - PHP-FPM Enhancements

## Goal
Improve PHP-FPM compatibility and observability by implementing management records, status/ping pages, and early request termination.

## Changes

### 1. FastCGI Protocol (`src/fcgi/protocol.rs`, `src/fcgi/request.rs`)
- Added `encode_params` to protocol module.
- Introduced `ResultRequest` enum to distinguish between application requests and management records.
- Updated `read_request` to return management records (request_id=0) immediately.

### 2. Management Records (`src/bin/php-fpm.rs`)
- Implemented `handle_get_values` to respond to `FCGI_GET_VALUES` queries.
- Supported keys: `FCGI_MAX_CONNS`, `FCGI_MAX_REQS`, `FCGI_MPXS_CONNS`.

### 3. Observability (`src/bin/php-fpm.rs`)
- Added `FpmMetrics` struct with shared atomic counters (`accepted_conn`, `active_requests`) and `start_time`.
- Implemented `/status` page providing uptime, connection counts, and worker activity.
- Implemented `/ping` page for health checks.
- Logic is intercepted in `execute_php` based on `REQUEST_URI`.

### 4. Integration Tests (`tests/fpm_integration_test.rs`)
- Added `test_fpm_get_values`: Verifies protocol-level management records.
- Added `test_fpm_status_page`: Verifies metrics reporting.
- Added `test_fpm_ping_page`: Verifies health check endpoint.

## Verification
- All integration tests pass.
- `cargo check` passes.
