//! SAPI (Server API) Functions
//!
//! Reference: $PHP_SRC_PATH/main/SAPI.c
//! Reference: $PHP_SRC_PATH/main/php_main.h

use crate::core::value::{Handle, Val};
use crate::vm::engine::VM;
use std::rc::Rc;

/// php_sapi_name() - Returns the type of interface between web server and PHP
///
/// Reference: $PHP_SRC_PATH/main/SAPI.c - php_sapi_name()
///
/// Returns the SAPI (Server API) name as a string.
/// Common values: "cli", "fpm-fcgi", "apache2handler", "cgi-fcgi", etc.
pub fn php_sapi_name(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if !args.is_empty() {
        return Err(format!("php_sapi_name() expects exactly 0 parameters, {} given", args.len()));
    }

    // Return "cli" for now - the most common SAPI mode
    // TODO: Track actual SAPI mode in VM or context
    let sapi_name = "cli";

    let val = Val::String(Rc::new(sapi_name.as_bytes().to_vec()));
    Ok(vm.arena.alloc(val))
}

/// php_uname() - Returns information about the operating system PHP is running on
///
/// Reference: $PHP_SRC_PATH/ext/standard/info.c - php_uname()
///
/// Syntax: php_uname(string $mode = "a"): string
pub fn php_uname(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    let mode = if args.is_empty() {
        "a"
    } else {
        match &vm.arena.get(args[0]).value {
            Val::String(s) => {
                std::str::from_utf8(s).unwrap_or("a")
            }
            _ => "a",
        }
    };

    let uname_info = match mode {
        "s" => std::env::consts::OS,
        "n" => "localhost", // hostname - simplified
        "r" => "", // release - not easily available in Rust
        "v" => "", // version - not easily available in Rust
        "m" => std::env::consts::ARCH,
        _ => {
            // "a" or default - all info
            &format!("{} localhost {} {}",
                std::env::consts::OS,
                std::env::consts::ARCH,
                std::env::consts::FAMILY
            )
        }
    };

    let val = Val::String(Rc::new(uname_info.as_bytes().to_vec()));
    Ok(vm.arena.alloc(val))
}

/// getmypid() - Gets PHP's process ID
///
/// Reference: $PHP_SRC_PATH/ext/standard/proc_open.c
pub fn php_getmypid(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if !args.is_empty() {
        return Err(format!("getmypid() expects exactly 0 parameters, {} given", args.len()));
    }

    let pid = std::process::id() as i64;
    let val = Val::Int(pid);
    Ok(vm.arena.alloc(val))
}

/// set_time_limit() - Limits the maximum execution time
///
/// Reference: $PHP_SRC_PATH/ext/standard/basic_functions.c - set_time_limit()
///
/// Note: This is a simplified implementation. PHP's version interacts with the Zend engine's
/// timeout mechanism. We currently don't enforce this limit.
pub fn php_set_time_limit(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() != 1 {
        return Err(format!("set_time_limit() expects exactly 1 parameter, {} given", args.len()));
    }

    let _seconds = match &vm.arena.get(args[0]).value {
        Val::Int(i) => *i,
        Val::Float(f) => *f as i64,
        Val::String(s) => {
            let s_str = String::from_utf8_lossy(s);
            s_str.parse::<i64>().unwrap_or(0)
        }
        Val::Bool(b) => if *b { 1 } else { 0 },
        Val::Null => 0,
        _ => return Err("set_time_limit() expects parameter 1 to be int".to_string()),
    };

    // TODO: Actually enforce time limits in the VM
    // For now, we just acknowledge the setting and return true

    Ok(vm.arena.alloc(Val::Bool(true)))
}

/// ignore_user_abort() - Set whether a client disconnect should abort script execution
///
/// Reference: $PHP_SRC_PATH/ext/standard/head.c
pub fn php_ignore_user_abort(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() > 1 {
        return Err(format!("ignore_user_abort() expects at most 1 parameter, {} given", args.len()));
    }

    // Get current setting (simplified - we don't track this yet)
    let current = 0i64;

    if !args.is_empty() {
        // Set new value
        let _new_value = match &vm.arena.get(args[0]).value {
            Val::Bool(b) => if *b { 1 } else { 0 },
            Val::Int(i) => *i,
            _ => 0,
        };
        // TODO: Store this setting in VM context
    }

    Ok(vm.arena.alloc(Val::Int(current)))
}

/// connection_aborted() - Check whether client disconnected
///
/// Reference: $PHP_SRC_PATH/ext/standard/head.c
pub fn php_connection_aborted(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if !args.is_empty() {
        return Err(format!("connection_aborted() expects exactly 0 parameters, {} given", args.len()));
    }

    // Simplified: always return 0 (not aborted)
    // TODO: Track actual connection status in SAPI layer
    Ok(vm.arena.alloc(Val::Int(0)))
}

/// connection_status() - Returns connection status bitfield
///
/// Reference: $PHP_SRC_PATH/ext/standard/head.c
pub fn php_connection_status(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if !args.is_empty() {
        return Err(format!("connection_status() expects exactly 0 parameters, {} given", args.len()));
    }

    // Simplified: always return 0 (NORMAL)
    // Constants: NORMAL=0, ABORTED=1, TIMEOUT=2
    Ok(vm.arena.alloc(Val::Int(0)))
}
