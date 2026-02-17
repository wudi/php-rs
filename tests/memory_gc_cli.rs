use php_rs::core::value::Val;
use php_rs::runtime::context::EngineBuilder;
use php_rs::sapi::SapiMode;
use php_rs::vm::engine::VM;

#[test]
fn gc_collects_unreachable_allocations() {
    let engine = EngineBuilder::new().with_core_extensions().build().unwrap();
    let mut vm = VM::new_with_sapi(engine, SapiMode::Cli);

    // Allocate many values that are NOT stored in any root
    for _ in 0..1000 {
        vm.arena.alloc(Val::Int(1));
    }

    // After allocation, heap has 1000+ values (plus superglobals from init)
    let before = vm.arena.len();
    assert!(before >= 1000);

    // GC should collect unreachable allocations
    // We need to provide roots - collect from the VM state
    vm.collect_garbage();

    // Most of the 1000 allocations should be collected since they're
    // not referenced from any VM root
    let after = vm.arena.len();
    assert!(
        after < before,
        "Expected GC to collect unreachable objects: before={}, after={}",
        before,
        after
    );
}

#[test]
fn gc_preserves_global_variables() {
    let engine = EngineBuilder::new().with_core_extensions().build().unwrap();
    let mut vm = VM::new_with_sapi(engine, SapiMode::Cli);

    // Allocate a value and store it in globals (making it a root)
    let sym = vm.context.interner.intern(b"test_var");
    let handle = vm.arena.alloc(Val::Int(42));
    vm.context.globals.insert(sym, handle);

    // GC should preserve the global variable
    vm.collect_garbage();

    // The global should still be accessible
    assert_eq!(vm.arena.get(handle).value, Val::Int(42));
}
