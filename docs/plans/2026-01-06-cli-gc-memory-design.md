# CLI GC (Epoch) + SAPI-Switched Memory Model Design

## Goals
- Provide an open memory allocation API for extensions/packages.
- Support two allocation policies: FPM request-scoped arena and CLI epoch-based GC.
- Keep PHP behavior unchanged while improving long-running CLI memory behavior.

## Architecture Overview
- The VM owns a `MemoryManager` that selects a policy based on SAPI mode at VM creation.
- Allocation API is public and policy-agnostic (extensions do not depend on GC internals).
- RequestContext contributes roots but does not own memory policy.

## Components
- `MemoryManager` (VM-owned): routes allocations to the active policy and tracks thresholds.
- `ArenaPolicy` (FPM): bump allocation, bulk free at request end.
- `EpochGcPolicy` (CLI): epoch-based reclamation using `crossbeam-epoch`.
- `GcRootProvider` trait: allows RequestContext/VM subsystems to enumerate roots.

## Root Set (CLI)
- VM stack frames (locals/temps)
- Globals
- RequestContext data (constants, classes, interner, extension data)

## Allocation API (Public)
- `alloc_bytes(size) -> *mut u8` or equivalent handle
- `alloc_val(Val) -> Handle`
- Typed helpers for extensions to allocate stable buffers
- Same API routes to arena or GC depending on SAPI mode

## GC Mechanics (CLI Epoch-Based)
- Use `crossbeam-epoch` to manage reclamation epochs for GC-managed runtime values.
- Each VM run-loop iteration enters a pinned epoch guard; allocations are tagged with the current epoch.
- Retire unreachable objects by deferring destruction until all active guards have advanced past the retire epoch.
- CLI remains long-lived; collection is paced by allocation thresholds that trigger reclamation cycles.
- FPM does not use epoch GC; it frees request memory via arena reset on request end.

## Error Handling
- On allocation failure, attempt a reclamation cycle; if still failing, return `VmError::OutOfMemory`.
- Ensure extension allocation APIs fail fast with clear errors when memory cannot be reclaimed.

## Testing
- Unit tests: epoch guard usage, retire timing, and reclamation safety.
- Root enumeration tests for globals and RequestContext data.
- Integration tests: long-running CLI script keeps memory stable; FPM request frees allocations.
- Extension-style test for allocation API lifetime across SAPI modes.

## Dependencies
- Add `crossbeam-epoch` for CLI epoch-based reclamation.

## Open Questions
- Threshold tuning for reclamation cycles in CLI mode.
- Whether CLI should expose manual reclamation triggers for debugging/testing.
