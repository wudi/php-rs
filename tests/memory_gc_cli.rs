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
