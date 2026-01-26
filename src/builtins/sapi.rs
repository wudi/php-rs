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

/// ini_parse_quantity() - Parse a byte quantity string
///
/// Reference: $PHP_SRC_PATH/Zend/zend_ini.c - zend_ini_parse_quantity()
///
/// Parses a configuration quantity string with optional multiplier suffix:
/// - K/k for kilobytes (1024 bytes)
/// - M/m for megabytes (1024^2 bytes)
/// - G/g for gigabytes (1024^3 bytes)
///
/// Also supports hex (0x), octal (0o), and binary (0b) prefixes.
/// Floats are truncated to integer part.
///
/// Examples: "128M" -> 134217728, "1G" -> 1073741824, "512K" -> 524288
pub fn php_ini_parse_quantity(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    use crate::vm::engine::ErrorLevel;
    
    if args.len() != 1 {
        return Err(format!("ini_parse_quantity() expects exactly 1 parameter, {} given", args.len()));
    }

    let input = match &vm.arena.get(args[0]).value {
        Val::String(s) => String::from_utf8_lossy(s).to_string(),
        Val::Int(i) => return Ok(vm.arena.alloc(Val::Int(*i))),
        Val::Float(f) => return Ok(vm.arena.alloc(Val::Int(*f as i64))),
        _ => return Err("ini_parse_quantity() expects parameter 1 to be string".to_string()),
    };

    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(vm.arena.alloc(Val::Int(0)));
    }

    // Handle sign
    let (is_negative, mut pos) = if trimmed.starts_with('-') {
        (true, 1)
    } else if trimmed.starts_with('+') {
        (false, 1)
    } else {
        (false, 0)
    };

    let rest = &trimmed[pos..];
    
    // Check for base prefix (0x, 0o, 0b)
    let (base, digits_start) = if rest.starts_with("0x") || rest.starts_with("0X") {
        (16, 2)
    } else if rest.starts_with("0o") || rest.starts_with("0O") {
        (8, 2)
    } else if rest.starts_with("0b") || rest.starts_with("0B") {
        (2, 2)
    } else {
        (10, 0)
    };
    
    pos += digits_start;
    let rest = &trimmed[pos..];
    
    // Find where the numeric portion ends (digits or decimal point)
    let mut digit_end = 0;
    let mut has_decimal = false;
    for (i, ch) in rest.chars().enumerate() {
        if ch == '.' && !has_decimal && base == 10 {
            has_decimal = true;
            digit_end = i + 1;
        } else if (base == 10 && ch.is_ascii_digit()) ||
                  (base == 16 && ch.is_ascii_hexdigit()) ||
                  (base == 8 && ch >= '0' && ch <= '7') ||
                  (base == 2 && (ch == '0' || ch == '1')) {
            digit_end = i + 1;
        } else {
            break;
        }
    }

    if digit_end == 0 {
        vm.trigger_error(
            ErrorLevel::Warning,
            &format!("Invalid quantity \"{}\": no valid leading digits, interpreting as \"0\" for backwards compatibility", trimmed)
        );
        return Ok(vm.arena.alloc(Val::Int(0)));
    }

    let number_part = &rest[..digit_end];
    
    // Parse the number (handle floats by truncating to integer)
    let number: i64 = if has_decimal {
        // For floats, just take the integer part
        let float_str = number_part;
        match float_str.parse::<f64>() {
            Ok(f) => f.trunc() as i64,
            Err(_) => {
                vm.trigger_error(
                    ErrorLevel::Warning,
                    &format!("Invalid quantity \"{}\": parse error, using 0", trimmed)
                );
                return Ok(vm.arena.alloc(Val::Int(0)));
            }
        }
    } else {
        match i64::from_str_radix(number_part, base) {
            Ok(n) => n,
            Err(_) => {
                vm.trigger_error(
                    ErrorLevel::Warning,
                    &format!("Invalid quantity \"{}\": number overflow, using 0", trimmed)
                );
                return Ok(vm.arena.alloc(Val::Int(0)));
            }
        }
    };

    // Skip whitespace between number and suffix
    let suffix_start = rest[digit_end..].trim_start();
    
    if suffix_start.is_empty() {
        // No suffix
        let result = if is_negative { -number } else { number };
        return Ok(vm.arena.alloc(Val::Int(result)));
    }

    let trimmed_suffix = suffix_start.trim();
    
    // Find the LAST occurrence of a valid multiplier (k/m/g) scanning from the end
    let last_valid_multiplier = trimmed_suffix.chars().rev()
        .find(|&c| matches!(c, 'k' | 'K' | 'm' | 'M' | 'g' | 'G'));
    
    // The last character (what PHP reports in errors for multi-char suffixes)
    let last_char = trimmed_suffix.chars().last().unwrap();
    
    let (has_valid_multiplier, multiplier_char) = match last_valid_multiplier {
        Some(c) => (true, c),
        None => (false, last_char),
    };
    
    // Check if it's a simple single-character suffix or multi-character
    let is_single_char = trimmed_suffix.len() == 1;
    
    if !has_valid_multiplier {
        // No valid multiplier found
        vm.trigger_error(
            ErrorLevel::Warning,
            &format!("Invalid quantity \"{}\": unknown multiplier \"{}\", interpreting as \"{}\" for backwards compatibility", 
                trimmed, last_char, number)
        );
        let result = if is_negative { -number } else { number };
        return Ok(vm.arena.alloc(Val::Int(result)));
    }
    
    let factor: i64 = match multiplier_char {
        'k' | 'K' => 1024,
        'm' | 'M' => 1024 * 1024,
        'g' | 'G' => 1024 * 1024 * 1024,
        _ => unreachable!(),
    };
    
    // If multi-character suffix, emit warning
    if !is_single_char {
        // Check if the last char is the same as the multiplier we found
        if last_char == multiplier_char {
            // e.g., "1gb" where last char is 'b' (invalid) but we found 'g'
            vm.trigger_error(
                ErrorLevel::Warning,
                &format!("Invalid quantity \"{}\": unknown multiplier \"{}\", interpreting as \"{}\" for backwards compatibility",
                    trimmed, last_char, number)
            );
            // Don't use the multiplier - just return the number
            let result = if is_negative { -number } else { number };
            return Ok(vm.arena.alloc(Val::Int(result)));
        } else {
            // e.g., "14.2bm" - we have a valid multiplier 'm' but also junk 'b'
            vm.trigger_error(
                ErrorLevel::Warning,
                &format!("Invalid quantity \"{}\", interpreting as \"{}{}\" for backwards compatibility",
                    trimmed, number, multiplier_char)
            );
        }
    }

    // Calculate result with overflow check
    let result = match number.checked_mul(factor) {
        Some(val) => if is_negative { -val } else { val },
        None => {
            vm.trigger_error(
                ErrorLevel::Warning,
                &format!("Invalid quantity \"{}\": value is out of range, using overflow result for backwards compatibility", trimmed)
            );
            // Return the overflowed value (wrapping multiply)
            let val = number.wrapping_mul(factor);
            if is_negative { -val } else { val }
        }
    };

    Ok(vm.arena.alloc(Val::Int(result)))
}
