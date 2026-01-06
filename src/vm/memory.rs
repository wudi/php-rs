use crate::core::heap::Arena;
use crate::core::value::{Handle, Val, Zval};
use crate::sapi::SapiMode;

pub trait HeapPolicy {
    fn alloc(&mut self, val: Val) -> Handle;
    fn get(&self, h: Handle) -> &Zval;
    fn get_mut(&mut self, h: Handle) -> &mut Zval;
    fn free(&mut self, h: Handle);
    fn len(&self) -> usize;
    fn name(&self) -> &'static str;
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
}

pub struct CliArenaPolicy {
    arena: Arena,
}

impl CliArenaPolicy {
    pub fn new() -> Self {
        Self {
            arena: Arena::new(),
        }
    }
}

impl HeapPolicy for CliArenaPolicy {
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
        "cli-arena"
    }
}

pub struct VmHeap {
    policy: Box<dyn HeapPolicy>,
}

impl VmHeap {
    pub fn new(mode: SapiMode) -> Self {
        match mode {
            SapiMode::FpmFcgi => Self {
                policy: Box::new(ArenaPolicy::new()),
            },
            SapiMode::Cli => Self {
                policy: Box::new(CliArenaPolicy::new()),
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
    fn heap_policy_switches_by_sapi() {
        let cli_heap = VmHeap::new(SapiMode::Cli);
        let fpm_heap = VmHeap::new(SapiMode::FpmFcgi);
        assert_ne!(cli_heap.policy_name(), fpm_heap.policy_name());
    }
}
