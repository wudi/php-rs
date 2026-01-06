# CLI Epoch GC Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace CLI’s long-lived arena with an epoch-based reclamation heap (using `crossbeam-epoch`) while keeping FPM request-scoped arenas, and expose a public allocation API for extensions.

**Architecture:** VM owns a `VmHeap` abstraction with SAPI-selected policy (`ArenaPolicy` for FPM, `EpochPolicy` for CLI). `RequestContext` exposes a memory allocation API that delegates to the VM heap. Epoch GC is used to defer destruction of unreachable allocations in CLI mode after explicit reclamation cycles.

**Tech Stack:** Rust, `crossbeam-epoch`, existing `Arena` and VM runtime.

### Task 1: Validate constraints and align with PHP lifecycle

**Files:**
- Reference: `$PHP_SRC_PATH/Zend/zend_gc.c`
- Reference: `$PHP_SRC_PATH/Zend/zend_alloc.c`
- Notes: `docs/plans/2026-01-06-cli-epoch-gc-implementation-plan.md`

**Step 1: Review PHP GC and request lifecycle**
- Open `$PHP_SRC_PATH/Zend/zend_gc.c` and `$PHP_SRC_PATH/Zend/zend_alloc.c`.
- Confirm PHP’s request-scoped memory freeing in FPM and long-lived process behavior in CLI.

**Step 2: Verify `crossbeam-epoch` constraints**
- Run: `cargo doc -p crossbeam-epoch --no-deps`
- Confirm `defer_destroy`/`Collector` usage supports non-`Send` values (or note required wrapper).

**Step 3: Record constraints**
- Add a short bullet list to this plan under **Open Questions** if constraints require refactors.

### Task 2: Add heap abstraction and arena policy

**Files:**
- Create: `src/vm/memory.rs`
- Modify: `src/vm/engine.rs`
- Modify: `src/core/heap.rs`
- Test: `src/vm/memory.rs`

**Step 1: Write failing unit test for heap abstraction**
```rust
#[test]
fn heap_alloc_get_roundtrip() {
    let mut heap = VmHeap::new(crate::sapi::SapiMode::FpmFcgi);
    let handle = heap.alloc(Val::Int(42));
    assert_eq!(heap.get(handle).value, Val::Int(42));
}
```

**Step 2: Run test to verify it fails**
Run: `cargo test heap_alloc_get_roundtrip -q`
Expected: FAIL (undefined `VmHeap`).

**Step 3: Implement `VmHeap` + `ArenaPolicy` skeleton**
```rust
pub trait HeapPolicy {
    fn alloc(&mut self, val: Val) -> Handle;
    fn get(&self, h: Handle) -> &Zval;
    fn get_mut(&mut self, h: Handle) -> &mut Zval;
    fn free(&mut self, h: Handle);
    fn len(&self) -> usize;
}

pub struct VmHeap {
    policy: Box<dyn HeapPolicy>,
}

impl VmHeap {
    pub fn new(mode: crate::sapi::SapiMode) -> Self {
        match mode {
            crate::sapi::SapiMode::FpmFcgi => Self { policy: Box::new(ArenaPolicy::new()) },
            crate::sapi::SapiMode::Cli => Self { policy: Box::new(ArenaPolicy::new()) },
        }
    }

    pub fn alloc(&mut self, val: Val) -> Handle { self.policy.alloc(val) }
    pub fn get(&self, h: Handle) -> &Zval { self.policy.get(h) }
    pub fn get_mut(&mut self, h: Handle) -> &mut Zval { self.policy.get_mut(h) }
    pub fn free(&mut self, h: Handle) { self.policy.free(h) }
    pub fn len(&self) -> usize { self.policy.len() }
}
```

**Step 4: Wire `VmHeap` into VM**
```rust
// src/vm/engine.rs
pub struct VM {
    pub arena: VmHeap,
    // ...
}
```
Update constructors to call `VmHeap::new(...)`.

**Step 5: Run test to verify it passes**
Run: `cargo test heap_alloc_get_roundtrip -q`
Expected: PASS.

**Step 6: Commit**
```bash
git add src/vm/memory.rs src/vm/engine.rs src/core/heap.rs
git commit -m "Add VM heap abstraction"
```

### Task 3: Add SAPI-based heap selection and wiring

**Files:**
- Modify: `src/vm/engine.rs`
- Modify: `src/bin/php.rs`
- Modify: `src/bin/php-fpm.rs`
- Modify: `src/vm/executor.rs`

**Step 1: Write failing unit test for CLI/FPM selection**
```rust
#[test]
fn heap_policy_switches_by_sapi() {
    let cli_heap = VmHeap::new(crate::sapi::SapiMode::Cli);
    let fpm_heap = VmHeap::new(crate::sapi::SapiMode::FpmFcgi);
    assert_ne!(cli_heap.policy_name(), fpm_heap.policy_name());
}
```

**Step 2: Run test to verify it fails**
Run: `cargo test heap_policy_switches_by_sapi -q`
Expected: FAIL (missing `policy_name`).

**Step 3: Add `policy_name` and `VM::new_with_sapi`**
```rust
impl VmHeap {
    pub fn policy_name(&self) -> &'static str { self.policy.name() }
}

impl VM {
    pub fn new_with_sapi(engine: Arc<EngineContext>, mode: crate::sapi::SapiMode) -> Self {
        let context = RequestContext::new(engine);
        Self::new_with_context_and_sapi(context, mode)
    }
}
```
Update CLI to call `VM::new_with_sapi(..., SapiMode::Cli)` and FPM to use `SapiMode::FpmFcgi`.

**Step 4: Run test to verify it passes**
Run: `cargo test heap_policy_switches_by_sapi -q`
Expected: PASS.

**Step 5: Commit**
```bash
git add src/vm/engine.rs src/bin/php.rs src/bin/php-fpm.rs src/vm/executor.rs
git commit -m "Wire VM heap selection by SAPI"
```

### Task 4: Implement `EpochPolicy` for CLI using `crossbeam-epoch`

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/vm/memory.rs`
- Test: `src/vm/memory.rs`

**Step 1: Add failing test for epoch retire**
```rust
#[test]
fn epoch_policy_retires_on_free() {
    let mut heap = VmHeap::new(crate::sapi::SapiMode::Cli);
    let h = heap.alloc(Val::Int(1));
    heap.free(h);
    assert_eq!(heap.len(), 0);
}
```

**Step 2: Run test to verify it fails**
Run: `cargo test epoch_policy_retires_on_free -q`
Expected: FAIL (CLI still arena-backed or `len` incorrect).

**Step 3: Add dependency**
```toml
# Cargo.toml
crossbeam-epoch = "0.9"
```

**Step 4: Implement `EpochPolicy`**
```rust
pub struct EpochPolicy {
    collector: crossbeam_epoch::Collector,
    slots: Vec<Option<std::ptr::NonNull<Zval>>>,
    free: Vec<usize>,
}

impl HeapPolicy for EpochPolicy {
    fn alloc(&mut self, val: Val) -> Handle {
        let boxed = Box::new(Zval { value: val, is_ref: false });
        let ptr = unsafe { std::ptr::NonNull::new_unchecked(Box::into_raw(boxed)) };
        if let Some(idx) = self.free.pop() {
            self.slots[idx] = Some(ptr);
            return Handle(idx as u32);
        }
        let idx = self.slots.len();
        self.slots.push(Some(ptr));
        Handle(idx as u32)
    }

    fn get(&self, h: Handle) -> &Zval {
        let ptr = self.slots[h.0 as usize].expect("use-after-free handle");
        unsafe { ptr.as_ref() }
    }

    fn get_mut(&mut self, h: Handle) -> &mut Zval {
        let ptr = self.slots[h.0 as usize].expect("use-after-free handle");
        unsafe { ptr.as_mut() }
    }

    fn free(&mut self, h: Handle) {
        if let Some(ptr) = self.slots[h.0 as usize].take() {
            let guard = self.collector.pin();
            unsafe { guard.defer_destroy(ptr.as_ptr()) };
            self.free.push(h.0 as usize);
        }
    }

    fn len(&self) -> usize {
        self.slots.len() - self.free.len()
    }
}
```

**Step 5: Switch CLI heap to `EpochPolicy`**
```rust
match mode {
    crate::sapi::SapiMode::Cli => Self { policy: Box::new(EpochPolicy::new()) },
    crate::sapi::SapiMode::FpmFcgi => Self { policy: Box::new(ArenaPolicy::new()) },
}
```

**Step 6: Run test to verify it passes**
Run: `cargo test epoch_policy_retires_on_free -q`
Expected: PASS.

**Step 7: Commit**
```bash
git add Cargo.toml src/vm/memory.rs
git commit -m "Add epoch-based heap policy for CLI"
```

### Task 5: Expose public allocation API for extensions

**Files:**
- Modify: `src/runtime/context.rs`
- Modify: `src/vm/engine.rs`
- Modify: `src/runtime/extension.rs`
- Test: `tests/memory_api.rs`

**Step 1: Write failing test for allocation API**
```rust
#[test]
fn memory_api_alloc_bytes_roundtrip() {
    let engine = php_rs::runtime::context::EngineBuilder::new()
        .with_core_extensions()
        .build()
        .unwrap();
    let mut vm = php_rs::vm::engine::VM::new_with_sapi(engine, php_rs::sapi::SapiMode::Cli);
    let block = vm.context.alloc_bytes(16);
    assert_eq!(block.len(), 16);
}
```

**Step 2: Run test to verify it fails**
Run: `cargo test memory_api_alloc_bytes_roundtrip -q`
Expected: FAIL (no allocation API).

**Step 3: Add `MemoryBlock` and RequestContext API**
```rust
pub struct MemoryBlock {
    ptr: std::ptr::NonNull<u8>,
    len: usize,
}

impl MemoryBlock {
    pub fn as_ptr(&self) -> *mut u8 { self.ptr.as_ptr() }
    pub fn len(&self) -> usize { self.len }
}

impl RequestContext {
    pub fn alloc_bytes(&mut self, len: usize) -> MemoryBlock {
        self.memory_api.alloc_bytes(len)
    }
}
```
Add `memory_api` field to `RequestContext` and initialize from `VM`.

**Step 4: Run test to verify it passes**
Run: `cargo test memory_api_alloc_bytes_roundtrip -q`
Expected: PASS.

**Step 5: Commit**
```bash
git add src/runtime/context.rs src/vm/engine.rs src/runtime/extension.rs tests/memory_api.rs
git commit -m "Expose public memory allocation API"
```

### Task 6: Add reclamation trigger and CLI smoke test

**Files:**
- Modify: `src/vm/engine.rs`
- Modify: `src/vm/memory.rs`
- Test: `tests/memory_gc_cli.rs`

**Step 1: Write failing smoke test**
```rust
#[test]
fn cli_epoch_reclamation_does_not_leak() {
    let engine = php_rs::runtime::context::EngineBuilder::new()
        .with_core_extensions()
        .build()
        .unwrap();
    let mut vm = php_rs::vm::engine::VM::new_with_sapi(engine, php_rs::sapi::SapiMode::Cli);
    for _ in 0..1000 {
        let h = vm.arena.alloc(Val::Int(1));
        vm.arena.free(h);
    }
    assert!(vm.arena.len() < 1000);
}
```

**Step 2: Run test to verify it fails**
Run: `cargo test cli_epoch_reclamation_does_not_leak -q`
Expected: FAIL (no reclamation).

**Step 3: Implement reclamation trigger**
```rust
impl VmHeap {
    pub fn maybe_reclaim(&mut self) {
        self.policy.maybe_reclaim();
    }
}
```
Call `maybe_reclaim()` from the VM dispatch loop or after a fixed number of allocations.

**Step 4: Run test to verify it passes**
Run: `cargo test cli_epoch_reclamation_does_not_leak -q`
Expected: PASS.

**Step 5: Commit**
```bash
git add src/vm/engine.rs src/vm/memory.rs tests/memory_gc_cli.rs
git commit -m "Add CLI epoch reclamation trigger"
```

## Open Questions
- Does `crossbeam-epoch` allow deferring destruction of `Zval` values containing `Rc` without `Send`/`Sync`? If not, decide whether to wrap with `Arc` or isolate non-`Send` contents.
- Should reclamation be explicit (per opcode loop) or threshold-based (bytes allocated) for CLI?
- `crossbeam-epoch` can run deferred destructors on another thread; if we keep `Rc` inside `Val`, do we enforce single-threaded collection or migrate to `Arc` for epoch-managed values?
