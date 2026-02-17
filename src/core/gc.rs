//! Garbage Collection Infrastructure
//!
//! Provides mark-and-sweep cycle-detecting garbage collection for the VM heap.
//! Replaces the previous epoch-based reclamation with a proper tracing GC
//! that can detect and collect circular references.
//!
//! ## Architecture
//!
//! - `GcHeap`: Manages Zval storage with mark-and-sweep collection
//! - `Traceable`: Trait for types that contain Handle references
//! - Collection is triggered periodically from the VM execution loop
//!
//! ## References
//!
//! - gc-arena crate concepts (mark-and-sweep, incremental collection)
//! - PHP's reference counting + cycle collector: `$PHP_SRC_PATH/Zend/zend_gc.c`

use crate::core::value::{Handle, Val, Zval};
use crate::vm::frame::{GeneratorData, GeneratorState, SubIterator};
use std::cell::RefCell;

/// Trait for types that can enumerate their contained Handle references.
/// Used by the mark-and-sweep GC to trace the object graph.
pub trait Traceable {
    /// Call `tracer(handle)` for each Handle reference contained in this value.
    fn trace_handles(&self, tracer: &mut dyn FnMut(Handle));
}

/// Trace all handles reachable from a CallFrame (locals, this, generator, args).
fn trace_call_frame(frame: &crate::vm::frame::CallFrame, tracer: &mut dyn FnMut(Handle)) {
    for h in frame.locals.values() {
        tracer(*h);
    }
    if let Some(h) = frame.this {
        tracer(h);
    }
    if let Some(h) = frame.generator {
        tracer(h);
    }
    for h in frame.args.iter() {
        tracer(*h);
    }
}

/// Trace all handles reachable from GeneratorData (suspended frames, yielded values, etc.).
fn trace_generator_data(gen_data: &GeneratorData, tracer: &mut dyn FnMut(Handle)) {
    // Trace suspended/created/delegating call frames
    match &gen_data.state {
        GeneratorState::Created(frame)
        | GeneratorState::Suspended(frame)
        | GeneratorState::Delegating(frame) => {
            trace_call_frame(frame, tracer);
        }
        GeneratorState::Running | GeneratorState::Finished => {}
    }

    if let Some(h) = gen_data.current_val {
        tracer(h);
    }
    if let Some(h) = gen_data.current_key {
        tracer(h);
    }
    if let Some(h) = gen_data.sent_val {
        tracer(h);
    }
    match &gen_data.sub_iter {
        Some(SubIterator::Array { handle, .. }) => tracer(*handle),
        Some(SubIterator::Generator { handle, .. }) => tracer(*handle),
        None => {}
    }
}

impl Traceable for Val {
    fn trace_handles(&self, tracer: &mut dyn FnMut(Handle)) {
        match self {
            Val::Object(h) => tracer(*h),
            Val::Array(arr) => {
                for (_, h) in &arr.map {
                    tracer(*h);
                }
            }
            Val::ObjPayload(obj) => {
                for (_, h) in &obj.properties {
                    tracer(*h);
                }
                // Trace through opaque internals (e.g. GeneratorData)
                if let Some(internal) = &obj.internal {
                    if let Some(gen_data) = internal.downcast_ref::<RefCell<GeneratorData>>() {
                        trace_generator_data(&gen_data.borrow(), tracer);
                    }
                }
            }
            Val::ConstArray(arr) => {
                // ConstArrays may contain Vals that reference handles
                for (_, v) in arr.iter() {
                    v.trace_handles(tracer);
                }
            }
            // Scalar types don't contain handles
            Val::Null
            | Val::Bool(_)
            | Val::Int(_)
            | Val::Float(_)
            | Val::String(_)
            | Val::Resource(_)
            | Val::AppendPlaceholder
            | Val::Uninitialized => {}
        }
    }
}

impl Traceable for Zval {
    fn trace_handles(&self, tracer: &mut dyn FnMut(Handle)) {
        self.value.trace_handles(tracer);
    }
}

/// Mark-and-sweep garbage collected heap for PHP values.
///
/// Stores Zvals in a slot-based allocator with per-slot mark bits.
/// Collection traces from a provided root set and frees unreachable slots.
#[derive(Debug)]
pub struct GcHeap {
    /// Zval storage indexed by Handle(u32)
    storage: Vec<Option<Zval>>,
    /// Free slot indices for reuse
    free_slots: Vec<usize>,
    /// Mark bits for GC (one per slot)
    marks: Vec<bool>,
    /// Number of allocations since last collection
    alloc_debt: usize,
    /// Threshold for triggering automatic collection
    gc_threshold: usize,
    /// Total number of live objects (for len())
    live_count: usize,
}

impl GcHeap {
    pub fn new() -> Self {
        Self {
            storage: Vec::with_capacity(1024),
            free_slots: Vec::new(),
            marks: Vec::with_capacity(1024),
            alloc_debt: 0,
            gc_threshold: 1024, // Initial threshold
            live_count: 0,
        }
    }

    /// Allocate a new Zval in the heap, returning a Handle to it.
    pub fn alloc(&mut self, val: Val) -> Handle {
        let zval = Zval {
            value: val,
            is_ref: false,
        };

        self.alloc_debt += 1;
        self.live_count += 1;

        if let Some(idx) = self.free_slots.pop() {
            self.storage[idx] = Some(zval);
            self.marks[idx] = false;
            return Handle(idx as u32);
        }

        let idx = self.storage.len();
        self.storage.push(Some(zval));
        self.marks.push(false);
        Handle(idx as u32)
    }

    /// Get an immutable reference to a Zval by handle.
    ///
    /// # Panics
    /// Panics if the handle refers to a freed or out-of-bounds slot.
    pub fn get(&self, h: Handle) -> &Zval {
        self.storage[h.0 as usize]
            .as_ref()
            .expect("use-after-free: handle refers to collected slot")
    }

    /// Get a mutable reference to a Zval by handle.
    ///
    /// # Panics
    /// Panics if the handle refers to a freed or out-of-bounds slot.
    pub fn get_mut(&mut self, h: Handle) -> &mut Zval {
        self.storage[h.0 as usize]
            .as_mut()
            .expect("use-after-free: handle refers to collected slot")
    }

    /// Get the number of live (allocated, not freed) values.
    pub fn len(&self) -> usize {
        self.live_count
    }

    /// Check if the heap has no live values.
    pub fn is_empty(&self) -> bool {
        self.live_count == 0
    }

    /// Returns the policy name for compatibility with existing code.
    pub fn policy_name(&self) -> &'static str {
        "gc"
    }

    /// Check if garbage collection should be triggered based on allocation debt.
    pub fn should_collect(&self) -> bool {
        self.alloc_debt >= self.gc_threshold
    }

    /// Run mark-and-sweep garbage collection.
    ///
    /// Traces from the provided root handles, marking all reachable objects.
    /// Unreachable objects are freed and their slots returned to the free list.
    ///
    /// Returns the number of objects collected.
    pub fn collect(&mut self, roots: &[Handle]) -> usize {
        // Phase 1: Clear all marks
        for mark in self.marks.iter_mut() {
            *mark = false;
        }

        // Phase 2: Mark - trace from roots using iterative DFS
        let mut worklist: Vec<Handle> = roots.to_vec();

        while let Some(h) = worklist.pop() {
            let idx = h.0 as usize;
            if idx >= self.marks.len() || self.marks[idx] {
                continue;
            }
            self.marks[idx] = true;

            // Trace handles referenced by this slot's value
            if let Some(zval) = &self.storage[idx] {
                zval.trace_handles(&mut |child| {
                    let child_idx = child.0 as usize;
                    if child_idx < self.marks.len() && !self.marks[child_idx] {
                        worklist.push(child);
                    }
                });
            }
        }

        // Phase 3: Sweep - free unmarked slots
        let mut collected = 0;
        for i in 0..self.storage.len() {
            if !self.marks[i] && self.storage[i].is_some() {
                self.storage[i] = None;
                self.free_slots.push(i);
                collected += 1;
            }
        }

        self.live_count = self.live_count.saturating_sub(collected);
        self.alloc_debt = 0;

        // Adaptive threshold: grow if we're collecting often with few results
        if collected < self.gc_threshold / 4 {
            self.gc_threshold = (self.gc_threshold * 2).min(65536);
        } else if collected > self.gc_threshold / 2 {
            self.gc_threshold = (self.gc_threshold / 2).max(256);
        }

        collected
    }

    /// Opportunistic reclamation hint (replaces maybe_reclaim from EpochPolicy).
    /// With mark-and-sweep, this is a no-op unless roots are provided.
    /// Actual collection is triggered by `collect()` with roots.
    pub fn maybe_reclaim(&mut self) {
        // No-op: mark-and-sweep requires roots to collect.
        // Collection is triggered from the VM execution loop where roots are available.
    }
}

impl Default for GcHeap {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::value::{ArrayData, ArrayKey, ObjectData, Symbol, Val};
    use indexmap::IndexMap;
    use std::collections::HashSet;
    use std::rc::Rc;

    #[test]
    fn alloc_and_get_roundtrip() {
        let mut heap = GcHeap::new();
        let h = heap.alloc(Val::Int(42));
        assert_eq!(heap.get(h).value, Val::Int(42));
    }

    #[test]
    fn collect_unreachable() {
        let mut heap = GcHeap::new();
        let root = heap.alloc(Val::Int(1));
        let _unreachable = heap.alloc(Val::Int(2));
        assert_eq!(heap.len(), 2);

        let collected = heap.collect(&[root]);
        assert_eq!(collected, 1);
        assert_eq!(heap.len(), 1);
        assert_eq!(heap.get(root).value, Val::Int(1));
    }

    #[test]
    fn collect_circular_references() {
        let mut heap = GcHeap::new();

        // Create two objects that reference each other
        let obj_a = heap.alloc(Val::Null); // placeholder
        let obj_b = heap.alloc(Val::Null); // placeholder

        // obj_a -> obj_b (via Object handle)
        heap.get_mut(obj_a).value = Val::Object(obj_b);
        // obj_b -> obj_a (via Object handle)
        heap.get_mut(obj_b).value = Val::Object(obj_a);

        // Both are unreachable (no roots)
        let collected = heap.collect(&[]);
        assert_eq!(collected, 2);
        assert_eq!(heap.len(), 0);
    }

    #[test]
    fn collect_preserves_reachable_cycles() {
        let mut heap = GcHeap::new();

        // Create a cycle that IS reachable
        let root = heap.alloc(Val::Null);
        let child = heap.alloc(Val::Null);

        heap.get_mut(root).value = Val::Object(child);
        heap.get_mut(child).value = Val::Object(root);

        let collected = heap.collect(&[root]);
        assert_eq!(collected, 0);
        assert_eq!(heap.len(), 2);
    }

    #[test]
    fn collect_traces_through_arrays() {
        let mut heap = GcHeap::new();

        let val_handle = heap.alloc(Val::Int(42));
        let mut arr = ArrayData::new();
        arr.insert(ArrayKey::Int(0), val_handle);

        let arr_handle = heap.alloc(Val::Array(Rc::new(arr)));

        // Only root is the array; value inside should be preserved
        let collected = heap.collect(&[arr_handle]);
        assert_eq!(collected, 0);
        assert_eq!(heap.len(), 2);
    }

    #[test]
    fn collect_traces_through_objects() {
        let mut heap = GcHeap::new();

        let prop_handle = heap.alloc(Val::String(Rc::new(b"hello".to_vec())));

        let mut props = IndexMap::new();
        props.insert(Symbol(1), prop_handle);

        let obj = ObjectData {
            class: Symbol(0),
            properties: props,
            internal: None,
            dynamic_properties: HashSet::new(),
        };
        let obj_handle = heap.alloc(Val::ObjPayload(obj));

        let collected = heap.collect(&[obj_handle]);
        assert_eq!(collected, 0);
        assert_eq!(heap.len(), 2);
    }

    #[test]
    fn slot_reuse_after_collection() {
        let mut heap = GcHeap::new();
        let _h1 = heap.alloc(Val::Int(1));
        let _h2 = heap.alloc(Val::Int(2));
        let h3 = heap.alloc(Val::Int(3));

        // Collect with only h3 as root
        heap.collect(&[h3]);
        assert_eq!(heap.len(), 1);

        // Allocate new values - should reuse freed slots
        let h4 = heap.alloc(Val::Int(4));
        assert!(h4.0 < 3); // Should reuse slot 0 or 1
    }

    #[test]
    fn collect_traces_through_generator_internals() {
        use crate::vm::frame::{CallFrame, GeneratorData, GeneratorState};
        use std::cell::RefCell;
        use std::collections::HashMap;

        let mut heap = GcHeap::new();

        // Create a value held by the generator's suspended frame
        let local_val = heap.alloc(Val::Int(999));
        let yielded_val = heap.alloc(Val::String(Rc::new(b"yielded".to_vec())));

        // Build a suspended CallFrame with a local variable
        let mut locals = HashMap::new();
        locals.insert(Symbol(1), local_val);
        let frame = CallFrame {
            chunk: Rc::new(crate::compiler::chunk::CodeChunk::default()),
            func: None,
            ip: 0,
            locals,
            this: None,
            is_constructor: false,
            class_scope: None,
            called_scope: None,
            generator: None,
            discard_return: false,
            args: smallvec::SmallVec::new(),
            callsite_strict_types: false,
            stack_base: None,
            pending_finally: None,
        };

        let generator_data = GeneratorData {
            state: GeneratorState::Suspended(frame),
            current_val: Some(yielded_val),
            current_key: None,
            auto_key: 0,
            sub_iter: None,
            sent_val: None,
        };

        // Store generator as ObjPayload with internal data
        let obj = ObjectData {
            class: Symbol(0),
            properties: IndexMap::new(),
            internal: Some(Rc::new(RefCell::new(generator_data))),
            dynamic_properties: HashSet::new(),
        };
        let obj_handle = heap.alloc(Val::ObjPayload(obj));

        // Only root is the generator object; suspended locals + yielded value
        // should be preserved via tracing through internal data
        let collected = heap.collect(&[obj_handle]);
        assert_eq!(
            collected, 0,
            "Generator internals should keep their handles alive"
        );
        assert_eq!(heap.len(), 3); // obj + local_val + yielded_val
        assert_eq!(heap.get(local_val).value, Val::Int(999));
    }
}
