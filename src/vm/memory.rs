//! VM Memory Management
//!
//! Provides the heap allocation interface for the VM using mark-and-sweep GC.
//! Replaces the previous epoch-based and arena-based policies with a unified
//! GC heap that handles cycle detection and automatic deallocation.

use crate::core::gc::GcHeap;
use crate::core::value::{Handle, Val, Zval};
use std::rc::Rc;

/// VM heap wrapping the GC-managed heap.
///
/// This provides the same API surface as the previous VmHeap (alloc/get/get_mut)
/// but uses mark-and-sweep garbage collection instead of epoch-based reclamation.
pub struct VmHeap {
    heap: GcHeap,
}

impl VmHeap {
    pub fn new(_mode: crate::sapi::SapiMode) -> Self {
        Self {
            heap: GcHeap::new(),
        }
    }

    pub fn alloc(&mut self, val: Val) -> Handle {
        self.heap.alloc(val)
    }

    pub fn get(&self, h: Handle) -> &Zval {
        self.heap.get(h)
    }

    pub fn get_mut(&mut self, h: Handle) -> &mut Zval {
        self.heap.get_mut(h)
    }

    pub fn len(&self) -> usize {
        self.heap.len()
    }

    pub fn policy_name(&self) -> &'static str {
        self.heap.policy_name()
    }

    /// No-op for API compatibility. Actual GC collection is triggered
    /// by `collect_garbage()` with root handles from the VM.
    pub fn maybe_reclaim(&mut self) {
        self.heap.maybe_reclaim();
    }

    /// Check if garbage collection should be triggered.
    pub fn should_collect(&self) -> bool {
        self.heap.should_collect()
    }

    /// Run mark-and-sweep garbage collection with the given root handles.
    /// Returns the number of objects collected.
    pub fn collect(&mut self, roots: &[Handle]) -> usize {
        self.heap.collect(roots)
    }
}

/// A block of allocated memory for extension use.
pub struct MemoryBlock {
    data: Rc<Vec<u8>>,
}

impl MemoryBlock {
    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn as_ptr(&self) -> *const u8 {
        self.data.as_ptr()
    }
}

/// Public memory allocation API for extensions.
///
/// Simplified from the previous version: no longer needs heap binding
/// since extensions allocate byte buffers independently of the GC heap.
pub struct MemoryApi;

impl MemoryApi {
    pub fn new_unbound() -> Self {
        Self
    }

    pub fn bind(&mut self, _heap: &mut VmHeap) {
        // No-op: MemoryApi no longer needs heap binding for byte allocations
    }

    pub fn alloc_bytes(&mut self, len: usize) -> MemoryBlock {
        MemoryBlock {
            data: Rc::new(vec![0u8; len]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::value::Val;
    use crate::sapi::SapiMode;

    #[test]
    fn heap_alloc_get_roundtrip() {
        let mut heap = VmHeap::new(SapiMode::Cli);
        let handle = heap.alloc(Val::Int(42));
        assert_eq!(heap.get(handle).value, Val::Int(42));
    }

    #[test]
    fn heap_policy_name() {
        let heap = VmHeap::new(SapiMode::Cli);
        assert_eq!(heap.policy_name(), "gc");
    }

    #[test]
    fn gc_collects_unreachable() {
        let mut heap = VmHeap::new(SapiMode::Cli);
        let root = heap.alloc(Val::Int(1));
        let _dead = heap.alloc(Val::Int(2));

        let collected = heap.collect(&[root]);
        assert_eq!(collected, 1);
        assert_eq!(heap.len(), 1);
    }
}
