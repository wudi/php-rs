# CLI GC + SAPI-Switched Memory Model Design

## Goals
- Provide an open memory allocation API for extensions/packages.
- Support two allocation policies: FPM request-scoped arena and CLI incremental GC.
- Keep PHP behavior unchanged while improving long-running CLI memory behavior.

## Architecture Overview
- The VM owns a `MemoryManager` that selects a policy based on SAPI mode at VM creation.
- Allocation API is public and policy-agnostic (extensions do not depend on GC internals).
- RequestContext contributes roots but does not own memory policy.

## Components
- `MemoryManager` (VM-owned): routes allocations to the active policy and tracks thresholds.
- `ArenaPolicy` (FPM): bump allocation, bulk free at request end.
- `IncrementalGcPolicy` (CLI): tri-color incremental collector with a worklist, write barrier, and sweeping.
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

## GC Mechanics (CLI Incremental)
- Tri-color marking with a gray worklist.
- Automatic GC triggers when `allocated_since_gc` crosses a threshold.
- Safepoints in opcode dispatch/loop boundaries advance the collector in small slices.
- Write barrier on mutable container operations (arrays/objects) to maintain incremental invariants.

## Error Handling
- On allocation failure, attempt a full GC cycle; if still failing, return `VmError::OutOfMemory`.
- Unsafe mutations must go through guarded APIs to preserve write barrier correctness.

## Testing
- Unit tests: graph traversal, cycles, and write barrier behavior.
- Root enumeration tests for globals and RequestContext data.
- Integration tests: long-running CLI script keeps memory stable; FPM request frees allocations.
- Extension-style test for allocation API lifetime across SAPI modes.

## Open Questions
- Exact GC slice size and threshold tuning (initial values, adaptive strategy).
- Whether CLI should expose manual GC triggers for debugging/testing.
