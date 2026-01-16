use crate::core::heap::Arena;
use crate::core::value::{Handle, Val, Zval};
use crate::sapi::SapiMode;
use crossbeam_epoch as epoch;
use std::any::Any;
use std::ptr::NonNull;
use std::rc::Rc;

pub trait HeapPolicy {
    fn alloc(&mut self, val: Val) -> Handle;
    fn get(&self, h: Handle) -> &Zval;
    fn get_mut(&mut self, h: Handle) -> &mut Zval;
    fn free(&mut self, h: Handle);
    fn len(&self) -> usize;
    fn name(&self) -> &'static str;
    fn maybe_reclaim(&mut self);
}

pub struct ArenaPolicy {
    arena: Arena,
}

impl ArenaPolicy {
    pub fn new() -> Self {
        Self {
            arena: Arena::new(),
        }
    }
}

impl HeapPolicy for ArenaPolicy {
    fn alloc(&mut self, val: Val) -> Handle {
        self.arena.alloc(val)
    }

    fn get(&self, h: Handle) -> &Zval {
        self.arena.get(h)
    }

    fn get_mut(&mut self, h: Handle) -> &mut Zval {
        self.arena.get_mut(h)
    }

    fn free(&mut self, h: Handle) {
        self.arena.free(h);
    }

    fn len(&self) -> usize {
        self.arena.len()
    }

    fn name(&self) -> &'static str {
        "arena"
    }

    fn maybe_reclaim(&mut self) {}
}

pub struct EpochPolicy {
    collector: epoch::Collector,
    local: epoch::LocalHandle,
    slots: Vec<Option<NonNull<Zval>>>,
    free: Vec<usize>,
}

impl EpochPolicy {
    pub fn new() -> Self {
        let collector = epoch::Collector::new();
        let local = collector.register();
        Self {
            collector,
            local,
            slots: Vec::new(),
            free: Vec::new(),
        }
    }
}

impl HeapPolicy for EpochPolicy {
    fn alloc(&mut self, val: Val) -> Handle {
        let boxed = Box::new(Zval {
            value: val,
            is_ref: false,
        });
        let ptr = unsafe { NonNull::new_unchecked(Box::into_raw(boxed)) };

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
        let mut ptr = self.slots[h.0 as usize].expect("use-after-free handle");
        unsafe { ptr.as_mut() }
    }

    fn free(&mut self, h: Handle) {
        if let Some(ptr) = self.slots[h.0 as usize].take() {
            let guard = self.local.pin();
            let shared = epoch::Shared::from(ptr.as_ptr() as *const Zval);
            unsafe { guard.defer_destroy(shared) };
            self.free.push(h.0 as usize);
        }
    }

    fn len(&self) -> usize {
        self.slots.len().saturating_sub(self.free.len())
    }

    fn name(&self) -> &'static str {
        "epoch"
    }

    fn maybe_reclaim(&mut self) {
        let guard = self.local.pin();
        guard.flush();
    }
}

pub struct VmHeap {
    policy: Box<dyn HeapPolicy>,
}

impl VmHeap {
    pub fn new(mode: SapiMode) -> Self {
        match mode {
            SapiMode::FpmFcgi => Self {
                policy: Box::new(EpochPolicy::new()),
            },
            SapiMode::Cli => Self {
                policy: Box::new(EpochPolicy::new()),
            },
        }
    }

    pub fn alloc(&mut self, val: Val) -> Handle {
        self.policy.alloc(val)
    }

    pub fn get(&self, h: Handle) -> &Zval {
        self.policy.get(h)
    }

    pub fn get_mut(&mut self, h: Handle) -> &mut Zval {
        self.policy.get_mut(h)
    }

    pub fn free(&mut self, h: Handle) {
        self.policy.free(h);
    }

    pub fn len(&self) -> usize {
        self.policy.len()
    }

    pub fn policy_name(&self) -> &'static str {
        self.policy.name()
    }

    pub fn maybe_reclaim(&mut self) {
        self.policy.maybe_reclaim();
    }
}

pub struct MemoryBlock {
    data: Rc<Vec<u8>>,
    heap: Option<NonNull<VmHeap>>,
    handle: Option<Handle>,
}

impl MemoryBlock {
    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn as_ptr(&self) -> *const u8 {
        self.data.as_ptr()
    }
}

impl Drop for MemoryBlock {
    fn drop(&mut self) {
        if let (Some(handle), Some(mut heap)) = (self.handle, self.heap) {
            unsafe { heap.as_mut() }.free(handle);
        }
    }
}

pub struct MemoryApi {
    heap: Option<NonNull<VmHeap>>,
}

impl MemoryApi {
    pub fn new_unbound() -> Self {
        Self { heap: None }
    }

    pub fn bind(&mut self, heap: &mut VmHeap) {
        self.heap = Some(NonNull::from(heap));
    }

    pub fn alloc_bytes(&mut self, len: usize) -> MemoryBlock {
        let data = Rc::new(vec![0u8; len]);
        let heap_ptr = self.heap;
        let handle = if let Some(mut heap) = self.heap {
            let resource: Rc<dyn Any> = data.clone();
            Some(unsafe { heap.as_mut() }.alloc(Val::Resource(resource)))
        } else {
            None
        };

        MemoryBlock {
            data,
            heap: heap_ptr,
            handle,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::value::Val;

    #[test]
    fn heap_alloc_get_roundtrip() {
        let mut heap = VmHeap::new(SapiMode::FpmFcgi);
        let handle = heap.alloc(Val::Int(42));
        assert_eq!(heap.get(handle).value, Val::Int(42));
    }

    #[test]
    fn heap_policy_unified_across_sapi() {
        let cli_heap = VmHeap::new(SapiMode::Cli);
        let fpm_heap = VmHeap::new(SapiMode::FpmFcgi);
        assert_eq!(cli_heap.policy_name(), fpm_heap.policy_name());
        assert_eq!(cli_heap.policy_name(), "epoch");
    }

    #[test]
    fn epoch_policy_retires_on_free() {
        let mut heap = VmHeap::new(SapiMode::Cli);
        let handle = heap.alloc(Val::Int(1));
        heap.free(handle);
        assert_eq!(heap.len(), 0);
    }
}
