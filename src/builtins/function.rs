use crate::core::value::{ArrayKey, Handle, Val};
use crate::runtime::context::ShutdownFunction;
use crate::vm::engine::{ErrorLevel, VM};
use std::rc::Rc;

/// func_get_args() - Returns an array comprising a function's argument list
///
/// PHP Reference: https://www.php.net/manual/en/function.func-get-args.php
///
/// Returns an array in which each element is a copy of the corresponding
/// member of the current user-defined function's argument list.
pub fn php_func_get_args(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // Get the current frame
    let frame = vm.frames.last().ok_or_else(|| {
        "func_get_args(): Called from the global scope - no function context".to_string()
    })?;

    // In PHP, func_get_args() returns the actual arguments passed to the function,
    // not the parameter definitions. These are stored in frame.args.
    let mut result_array = indexmap::IndexMap::new();

    for (idx, &arg_handle) in frame.args.iter().enumerate() {
        let arg_val = vm.arena.get(arg_handle).value.clone();
        let key = ArrayKey::Int(idx as i64);
        let val_handle = vm.arena.alloc(arg_val);
        result_array.insert(key, val_handle);
    }

    Ok(vm
        .arena
        .alloc(Val::Array(Rc::new(crate::core::value::ArrayData::from(
            result_array,
        )))))
}

/// func_num_args() - Returns the number of arguments passed to the function
///
/// PHP Reference: https://www.php.net/manual/en/function.func-num-args.php
///
/// Gets the number of arguments passed to the function.
pub fn php_func_num_args(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let frame = vm.frames.last().ok_or_else(|| {
        "func_num_args(): Called from the global scope - no function context".to_string()
    })?;

    let count = frame.args.len() as i64;
    Ok(vm.arena.alloc(Val::Int(count)))
}

/// func_get_arg() - Return an item from the argument list
///
/// PHP Reference: https://www.php.net/manual/en/function.func-get-arg.php
///
/// Gets the specified argument from a user-defined function's argument list.
pub fn php_func_get_arg(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("func_get_arg() expects exactly 1 argument, 0 given".to_string());
    }

    let frame = vm.frames.last().ok_or_else(|| {
        "func_get_arg(): Called from the global scope - no function context".to_string()
    })?;

    let arg_num_val = &vm.arena.get(args[0]).value;
    let arg_num = match arg_num_val {
        Val::Int(i) => *i,
        _ => return Err("func_get_arg(): Argument #1 must be of type int".to_string()),
    };

    if arg_num < 0 {
        return Err(format!(
            "func_get_arg(): Argument #1 must be greater than or equal to 0"
        ));
    }

    let idx = arg_num as usize;
    if idx >= frame.args.len() {
        return Err(format!(
            "func_get_arg(): Argument #{} not passed to function",
            arg_num
        ));
    }

    let arg_handle = frame.args[idx];
    let arg_val = vm.arena.get(arg_handle).value.clone();
    Ok(vm.arena.alloc(arg_val))
}

/// function_exists() - Return TRUE if the given function has been defined
///
/// PHP Reference: https://www.php.net/manual/en/function.function-exists.php
pub fn php_function_exists(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() != 1 {
        return Err(format!(
            "function_exists() expects exactly 1 parameter, {} given",
            args.len()
        ));
    }

    let name_val = vm.arena.get(args[0]);
    let exists = match &name_val.value {
        Val::String(s) => function_exists_case_insensitive(vm, s.as_slice()),
        _ => {
            return Err("function_exists() expects parameter 1 to be string".to_string());
        }
    };
    Ok(vm.arena.alloc(Val::Bool(exists)))
}

/// is_callable() - Verify that the contents of a variable can be called as a function
pub fn php_is_callable(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("is_callable() expects at least 1 parameter, 0 given".to_string());
    }

    if args.len() > 3 {
        return Err(format!(
            "is_callable() expects at most 3 parameters, {} given",
            args.len()
        ));
    }

    let syntax_only = args
        .get(1)
        .map(|handle| vm.arena.get(*handle).value.to_bool())
        .unwrap_or(false);

    let target = vm.arena.get(args[0]);
    let callable = match &target.value {
        Val::String(name) => {
            if syntax_only {
                !name.is_empty()
            } else {
                function_exists_case_insensitive(vm, name.as_slice())
            }
        }
        _ => false,
    };

    Ok(vm.arena.alloc(Val::Bool(callable)))
}

fn function_exists_case_insensitive(vm: &VM, name_bytes: &[u8]) -> bool {
    let stripped = if name_bytes.starts_with(b"\\") {
        &name_bytes[1..]
    } else {
        name_bytes
    };

    let _lower_name: Vec<u8> = stripped.iter().map(|b| b.to_ascii_lowercase()).collect();

    // Check extension-registered functions in the registry
    if vm.context.engine.registry.get_function(stripped).is_some() {
        return true;
    }

    // Check user-defined functions
    vm.context
        .user_functions
        .keys()
        .any(|sym| match vm.context.interner.lookup(*sym) {
            Some(stored) => {
                let stored_stripped = if stored.starts_with(b"\\") {
                    &stored[1..]
                } else {
                    stored
                };
                stored_stripped.eq_ignore_ascii_case(stripped)
            }
            None => false,
        })
}

/// extension_loaded() - Find out whether an extension is loaded
///
/// For now we only report "core" and "standard" as available since this VM
/// doesn't ship other extensions yet.
pub fn php_extension_loaded(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() != 1 {
        return Err(format!(
            "extension_loaded() expects exactly 1 parameter, {} given",
            args.len()
        ));
    }

    let ext_val = vm.arena.get(args[0]);
    let ext_name = match &ext_val.value {
        Val::String(s) => s.as_slice(),
        _ => {
            return Err("extension_loaded() expects parameter 1 to be string".to_string());
        }
    };

    // Normalize to lowercase for case-insensitive comparison
    let ext_name_str = String::from_utf8_lossy(ext_name).to_lowercase();

    // Check extension registry first
    let is_loaded = vm.context.engine.registry.extension_loaded(&ext_name_str);

    // Fallback to hardcoded always-on extensions
    let is_loaded = is_loaded || {
        const ALWAYS_ON: [&str; 2] = ["core", "standard"];
        ALWAYS_ON.contains(&ext_name_str.as_str())
    };

    Ok(vm.arena.alloc(Val::Bool(is_loaded)))
}

/// assert() - Checks an assertion and reports a warning when it fails
pub fn php_assert(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("assert() expects at least 1 parameter".to_string());
    }

    let assertion_val = vm.arena.get(args[0]);
    let passed = assertion_val.value.to_bool();

    if !passed {
        let message = args
            .get(1)
            .and_then(|handle| match &vm.arena.get(*handle).value {
                Val::String(s) => Some(String::from_utf8_lossy(s).into_owned()),
                _ => None,
            });

        let warning = message
            .as_deref()
            .unwrap_or("Assertion failed without a message");
        vm.error_handler.report(ErrorLevel::Warning, warning);
    }

    Ok(vm.arena.alloc(Val::Bool(passed)))
}

/// call_user_func() - Call a user function given in the first parameter
pub fn php_call_user_func(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("call_user_func() expects at least 1 parameter".to_string());
    }

    let callback_handle = args[0];
    let func_args: smallvec::SmallVec<[Handle; 8]> = args[1..].iter().copied().collect();

    vm.call_callable(callback_handle, func_args)
        .map_err(|e| format!("call_user_func error: {:?}", e))
}

/// call_user_func_array() - Call a user function with parameters as an array
pub fn php_call_user_func_array(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() < 2 {
        return Err("call_user_func_array() expects exactly 2 parameters".to_string());
    }

    let callback_handle = args[0];
    let params_handle = args[1];

    // Extract array elements as arguments
    let func_args: smallvec::SmallVec<[Handle; 8]> = match &vm.arena.get(params_handle).value {
        Val::Array(arr) => arr.map.values().copied().collect(),
        _ => return Err("call_user_func_array() expects parameter 2 to be array".to_string()),
    };

    vm.call_callable(callback_handle, func_args)
        .map_err(|e| format!("call_user_func_array error: {:?}", e))
}

/// register_shutdown_function() - Register a function to be called on shutdown
///
/// PHP Reference: https://www.php.net/manual/en/function.register-shutdown-function.php
pub fn php_register_shutdown_function(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("register_shutdown_function() expects at least 1 parameter".to_string());
    }

    let callable = args[0];
    if !vm.is_callable(callable) {
        return Err(
            "register_shutdown_function(): Argument #1 ($callback) must be a valid callback"
                .to_string(),
        );
    }

    let shutdown_args = args[1..].to_vec();
    vm.context.shutdown_functions.push(ShutdownFunction {
        callable,
        args: shutdown_args,
    });

    Ok(vm.arena.alloc(Val::Null))
}

/// set_error_handler() - Sets a user-defined error handler function
///
/// PHP Reference: https://www.php.net/manual/en/function.set-error-handler.php
pub fn php_set_error_handler(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("set_error_handler() expects at least 1 parameter".to_string());
    }

    if args.len() > 2 {
        return Err(format!(
            "set_error_handler() expects at most 2 parameters, {} given",
            args.len()
        ));
    }

    let new_handler_handle = args[0];
    let new_handler = match &vm.arena.get(new_handler_handle).value {
        Val::Null => None,
        _ => {
            if !vm.is_callable(new_handler_handle) {
                return Err("set_error_handler(): Argument #1 ($callback) must be a valid callback"
                    .to_string());
            }
            Some(new_handler_handle)
        }
    };

    let error_type = args
        .get(1)
        .map(|handle| vm.arena.get(*handle).value.to_int() as u32)
        .unwrap_or(32767);

    let previous = vm.context.user_error_handler;
    let previous_reporting = vm.context.user_error_handler_reporting;
    vm.context
        .user_error_handler_stack
        .push((previous, previous_reporting));

    vm.context.user_error_handler = new_handler;
    vm.context.user_error_handler_reporting = error_type;

    if let Some(previous_handle) = previous {
        Ok(previous_handle)
    } else {
        Ok(vm.arena.alloc(Val::Null))
    }
}

/// restore_error_handler() - Restores the previous error handler
///
/// PHP Reference: https://www.php.net/manual/en/function.restore-error-handler.php
pub fn php_restore_error_handler(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    if let Some((handler, reporting)) = vm.context.user_error_handler_stack.pop() {
        vm.context.user_error_handler = handler;
        vm.context.user_error_handler_reporting = reporting;
        Ok(vm.arena.alloc(Val::Bool(true)))
    } else {
        vm.context.user_error_handler = None;
        Ok(vm.arena.alloc(Val::Bool(false)))
    }
}

/// trigger_error() - Generates a user-level error/warning/notice message
///
/// PHP Reference: https://www.php.net/manual/en/function.trigger-error.php
pub fn php_trigger_error(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("trigger_error() expects at least 1 parameter".to_string());
    }

    let message = match &vm.arena.get(args[0]).value {
        Val::String(s) => String::from_utf8_lossy(s).to_string(),
        _ => return Err("trigger_error() expects parameter 1 to be string".to_string()),
    };

    let error_type = args
        .get(1)
        .map(|handle| vm.arena.get(*handle).value.to_int())
        .unwrap_or(ErrorLevel::UserNotice.to_bitmask() as i64);

    let level = match error_type {
        256 => ErrorLevel::UserError,
        512 => ErrorLevel::UserWarning,
        1024 => ErrorLevel::UserNotice,
        16384 => ErrorLevel::Deprecated,
        _ => {
            return Err(
                "trigger_error(): Argument #2 must be one of E_USER_ERROR, E_USER_WARNING, E_USER_NOTICE, or E_USER_DEPRECATED"
                    .to_string(),
            );
        }
    };

    vm.report_error(level, &message);
    Ok(vm.arena.alloc(Val::Bool(true)))
}
