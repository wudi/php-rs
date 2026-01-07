use php_rs::runtime::context::EngineBuilder;
use php_rs::sapi::SapiMode;
use php_rs::vm::engine::VM;

#[test]
fn memory_api_alloc_bytes_roundtrip() {
    let engine = EngineBuilder::new().with_core_extensions().build().unwrap();
    let mut vm = VM::new_with_sapi(engine, SapiMode::Cli);
    let block = vm.context.alloc_bytes(16);
    assert_eq!(block.len(), 16);
}
