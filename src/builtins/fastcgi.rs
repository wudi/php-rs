use crate::core::value::{Handle, Val};
use crate::vm::engine::VM;

/// fastcgi_finish_request — Flushes all open output buffers to the client
///
/// This function flushes all open output buffers, sends the response headers
/// and content to the client, and finishes the FastCGI request.
/// The script continues running after this function returns.
pub fn fastcgi_finish_request(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    vm.finish_request().map_err(|e| e.to_string())?;
    Ok(vm.arena.alloc(Val::Bool(true)))
}
