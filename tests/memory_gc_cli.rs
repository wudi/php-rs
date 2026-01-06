use php_rs::core::value::Val;
use php_rs::runtime::context::EngineBuilder;
use php_rs::sapi::SapiMode;
use php_rs::vm::engine::VM;

#[test]
fn cli_epoch_reclamation_does_not_leak() {
    let engine = EngineBuilder::new()
        .with_core_extensions()
        .build()
        .unwrap();
    let mut vm = VM::new_with_sapi(engine, SapiMode::Cli);

    for _ in 0..1000 {
        let handle = vm.arena.alloc(Val::Int(1));
        vm.arena.free(handle);
        vm.arena.maybe_reclaim();
    }

    assert!(vm.arena.len() < 1000);
}

#[test]
fn cli_alloc_bytes_reclaims_after_drop() {
    let engine = EngineBuilder::new()
        .with_core_extensions()
        .build()
        .unwrap();
    let mut vm = VM::new_with_sapi(engine, SapiMode::Cli);
    let baseline = vm.arena.len();

    let blocks: Vec<_> = (0..1000).map(|_| vm.context.alloc_bytes(32)).collect();
    let allocated = vm.arena.len();
    assert!(allocated > baseline);

    drop(blocks);

    for _ in 0..3 {
        vm.arena.maybe_reclaim();
    }

    assert_eq!(vm.arena.len(), baseline);
}
