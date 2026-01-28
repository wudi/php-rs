use crate::core::value::{Handle, Val};
use crate::vm::engine::VM;

pub fn php_abs(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("abs() expects exactly 1 parameter".into());
    }

    let val = vm.arena.get(args[0]);
    match &val.value {
        Val::Int(i) => Ok(vm.arena.alloc(Val::Int(i.abs()))),
        Val::Float(f) => Ok(vm.arena.alloc(Val::Float(f.abs()))),
        Val::String(s) => {
            // String coercion: only in weak mode
            if vm.builtin_call_strict {
                return Err("abs(): Argument #1 must be of type int|float, string given".into());
            }
            // Weak mode: try to parse as number
            let s_str = String::from_utf8_lossy(s);
            if let Ok(i) = s_str.parse::<i64>() {
                Ok(vm.arena.alloc(Val::Int(i.abs())))
            } else if let Ok(f) = s_str.parse::<f64>() {
                Ok(vm.arena.alloc(Val::Float(f.abs())))
            } else {
                Ok(vm.arena.alloc(Val::Int(0)))
            }
        }
        Val::Bool(b) => {
            if vm.builtin_call_strict {
                return Err("abs(): Argument #1 must be of type int|float, bool given".into());
            }
            Ok(vm.arena.alloc(Val::Int(if *b { 1 } else { 0 })))
        }
        Val::Null => {
            if vm.builtin_call_strict {
                return Err("abs(): Argument #1 must be of type int|float, null given".into());
            }
            Ok(vm.arena.alloc(Val::Int(0)))
        }
        _ => {
            if vm.builtin_call_strict {
                Err(format!(
                    "abs(): Argument #1 must be of type int|float, {} given",
                    val.value.type_name()
                ))
            } else {
                Ok(vm.arena.alloc(Val::Int(0)))
            }
        }
    }
}

pub fn php_max(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("max() expects at least 1 parameter".into());
    }

    if args.len() == 1 {
        // Single array argument
        let val = vm.arena.get(args[0]);
        if let Val::Array(arr_rc) = &val.value {
            if arr_rc.map.is_empty() {
                return Err("max(): Array must contain at least one element".into());
            }
            let mut max_handle = *arr_rc.map.values().next().unwrap();
            for &handle in arr_rc.map.values().skip(1) {
                if compare_values(vm, handle, max_handle) > 0 {
                    max_handle = handle;
                }
            }
            return Ok(max_handle);
        }
    }

    // Multiple arguments
    let mut max_handle = args[0];
    for &handle in &args[1..] {
        if compare_values(vm, handle, max_handle) > 0 {
            max_handle = handle;
        }
    }
    Ok(max_handle)
}

pub fn php_min(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("min() expects at least 1 parameter".into());
    }

    if args.len() == 1 {
        // Single array argument
        let val = vm.arena.get(args[0]);
        if let Val::Array(arr_rc) = &val.value {
            if arr_rc.map.is_empty() {
                return Err("min(): Array must contain at least one element".into());
            }
            let mut min_handle = *arr_rc.map.values().next().unwrap();
            for &handle in arr_rc.map.values().skip(1) {
                if compare_values(vm, handle, min_handle) < 0 {
                    min_handle = handle;
                }
            }
            return Ok(min_handle);
        }
    }

    // Multiple arguments
    let mut min_handle = args[0];
    for &handle in &args[1..] {
        if compare_values(vm, handle, min_handle) < 0 {
            min_handle = handle;
        }
    }
    Ok(min_handle)
}

fn compare_values(vm: &VM, a: Handle, b: Handle) -> i32 {
    let a_val = vm.arena.get(a);
    let b_val = vm.arena.get(b);

    match (&a_val.value, &b_val.value) {
        (Val::Int(i1), Val::Int(i2)) => i1.cmp(i2) as i32,
        (Val::Float(f1), Val::Float(f2)) => {
            if f1 < f2 {
                -1
            } else if f1 > f2 {
                1
            } else {
                0
            }
        }
        (Val::Int(i), Val::Float(f)) | (Val::Float(f), Val::Int(i)) => {
            let i_f = *i as f64;
            if i_f < *f {
                -1
            } else if i_f > *f {
                1
            } else {
                0
            }
        }
        _ => 0,
    }
}

/// round(float $num, int $precision = 0, int $mode = PHP_ROUND_HALF_UP): float
/// Rounds a float
/// Reference: $PHP_SRC_PATH/ext/standard/math.c - PHP_FUNCTION(round)
pub fn php_round(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("round() expects at least 1 parameter".into());
    }

    let num = match &vm.arena.get(args[0]).value {
        Val::Float(f) => *f,
        Val::Int(i) => *i as f64,
        _ => return Err("round(): Argument #1 must be of type float".into()),
    };

    let precision = if args.len() > 1 {
        match &vm.arena.get(args[1]).value {
            Val::Int(i) => *i as i32,
            _ => 0,
        }
    } else {
        0
    };

    let result = if precision == 0 {
        num.round()
    } else if precision > 0 {
        let multiplier = 10_f64.powi(precision);
        (num * multiplier).round() / multiplier
    } else {
        let divisor = 10_f64.powi(-precision);
        (num / divisor).round() * divisor
    };

    Ok(vm.arena.alloc(Val::Float(result)))
}

/// floor(float $num): float
/// Round fractions down
/// Reference: $PHP_SRC_PATH/ext/standard/math.c - PHP_FUNCTION(floor)
pub fn php_floor(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() != 1 {
        return Err("floor() expects exactly 1 parameter".into());
    }

    let num = match &vm.arena.get(args[0]).value {
        Val::Float(f) => *f,
        Val::Int(i) => *i as f64,
        _ => return Err("floor(): Argument #1 must be of type float".into()),
    };

    Ok(vm.arena.alloc(Val::Float(num.floor())))
}

/// ceil(float $num): float
/// Round fractions up
/// Reference: $PHP_SRC_PATH/ext/standard/math.c - PHP_FUNCTION(ceil)
pub fn php_ceil(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() != 1 {
        return Err("ceil() expects exactly 1 parameter".into());
    }

    let num = match &vm.arena.get(args[0]).value {
        Val::Float(f) => *f,
        Val::Int(i) => *i as f64,
        _ => return Err("ceil(): Argument #1 must be of type float".into()),
    };

    Ok(vm.arena.alloc(Val::Float(num.ceil())))
}
