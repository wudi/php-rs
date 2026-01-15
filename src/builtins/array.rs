use crate::core::value::{ArrayData, ArrayKey, ConstArrayKey, Handle, Val};
use crate::vm::engine::VM;
use indexmap::IndexMap;
use smallvec::smallvec;
use std::collections::HashSet;
use std::rc::Rc;

pub fn php_count(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() != 1 {
        return Err("count() expects exactly 1 parameter".into());
    }

    let val = vm.arena.get(args[0]);
    let count = match &val.value {
        Val::Array(arr) => arr.map.len(),
        Val::Null => 0,
        Val::ConstArray(map) => map.len(),
        Val::Object(payload_handle) => {
            // Check if object implements Countable interface
            let payload = vm.arena.get(*payload_handle);
            if let Val::ObjPayload(obj_data) = &payload.value {
                let obj_class = obj_data.class;
                let countable_sym = vm.context.interner.intern(b"Countable");

                // Check if class or any parent implements Countable
                let implements_countable =
                    { obj_class == countable_sym || vm.is_subclass_of(obj_class, countable_sym) };

                if implements_countable {
                    let count_sym = vm.context.interner.intern(b"count");
                    // Call the count() method on the object
                    match vm.call_method_simple(args[0], count_sym) {
                        Ok(result_handle) => {
                            if let Val::Int(n) = vm.arena.get(result_handle).value {
                                return Ok(vm.arena.alloc(Val::Int(n)));
                            }
                            return Err("count() method must return an integer".into());
                        }
                        Err(e) => return Err(format!("Error calling count(): {}", e)),
                    }
                }
            }
            // In PHP, count() on non-array/non-Countable returns 1
            1
        }
        // In PHP, count() on non-array/non-Countable returns 1
        _ => 1,
    };

    Ok(vm.arena.alloc(Val::Int(count as i64)))
}

pub fn php_array_merge(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    let mut new_array = IndexMap::new();
    let mut next_int_key = 0;

    for (i, arg_handle) in args.iter().enumerate() {
        let val = vm.arena.get(*arg_handle);
        match &val.value {
            Val::Array(arr) => {
                for (key, value_handle) in arr.map.iter() {
                    match key {
                        ArrayKey::Int(_) => {
                            new_array.insert(ArrayKey::Int(next_int_key), *value_handle);
                            next_int_key += 1;
                        }
                        ArrayKey::Str(s) => {
                            new_array.insert(ArrayKey::Str(s.clone()), *value_handle);
                        }
                    }
                }
            }
            _ => {
                return Err(format!(
                    "array_merge(): Argument #{} is not an array",
                    i + 1
                ));
            }
        }
    }

    Ok(vm.arena.alloc(Val::Array(
        crate::core::value::ArrayData::from(new_array).into(),
    )))
}

pub fn php_array_search(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() < 2 || args.len() > 3 {
        return Err("array_search() expects 2 or 3 parameters".into());
    }

    let needle = vm.arena.get(args[0]).value.clone();
    let strict = if args.len() == 3 {
        vm.arena.get(args[2]).value.to_bool()
    } else {
        false
    };

    match &vm.arena.get(args[1]).value {
        Val::Array(arr) => {
            for (key, value_handle) in arr.map.iter() {
                let candidate = vm.arena.get(*value_handle).value.clone();
                if values_equal(&needle, &candidate, strict) {
                    let key_val = match key {
                        ArrayKey::Int(i) => Val::Int(*i),
                        ArrayKey::Str(s) => Val::String(s.clone()),
                    };
                    return Ok(vm.arena.alloc(key_val));
                }
            }
        }
        Val::ConstArray(map) => {
            for (key, val) in map.iter() {
                if values_equal(&needle, val, strict) {
                    let key_val = match key {
                        ConstArrayKey::Int(i) => Val::Int(*i),
                        ConstArrayKey::Str(s) => Val::String(s.clone()),
                    };
                    return Ok(vm.arena.alloc(key_val));
                }
            }
        }
        _ => return Err("array_search(): Argument #2 ($haystack) must be of type array".into()),
    }

    Ok(vm.arena.alloc(Val::Bool(false)))
}

pub fn php_array_keys(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() < 1 {
        return Err("array_keys() expects at least 1 parameter".into());
    }

    let keys: Vec<ArrayKey> = {
        let val = vm.arena.get(args[0]);
        let arr = match &val.value {
            Val::Array(arr) => arr,
            _ => return Err("array_keys() expects parameter 1 to be array".into()),
        };
        arr.map.keys().cloned().collect()
    };

    let mut keys_arr = IndexMap::new();
    let mut idx = 0;

    for key in keys {
        let key_val = match key {
            ArrayKey::Int(i) => Val::Int(i),
            ArrayKey::Str(s) => Val::String((*s).clone().into()),
        };
        let key_handle = vm.arena.alloc(key_val);
        keys_arr.insert(ArrayKey::Int(idx), key_handle);
        idx += 1;
    }

    Ok(vm.arena.alloc(Val::Array(
        crate::core::value::ArrayData::from(keys_arr).into(),
    )))
}

pub fn php_array_values(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() != 1 {
        return Err("array_values() expects exactly 1 parameter".into());
    }

    let val = vm.arena.get(args[0]);
    let arr = match &val.value {
        Val::Array(arr) => arr,
        _ => return Err("array_values() expects parameter 1 to be array".into()),
    };

    let mut values_arr = IndexMap::new();
    let mut idx = 0;

    for (_, value_handle) in arr.map.iter() {
        values_arr.insert(ArrayKey::Int(idx), *value_handle);
        idx += 1;
    }

    Ok(vm.arena.alloc(Val::Array(
        crate::core::value::ArrayData::from(values_arr).into(),
    )))
}

pub fn php_in_array(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() < 2 || args.len() > 3 {
        return Err("in_array() expects 2 or 3 parameters".into());
    }

    let needle = vm.arena.get(args[0]).value.clone();

    let haystack = match &vm.arena.get(args[1]).value {
        Val::Array(arr) => arr,
        _ => return Err("in_array(): Argument #2 ($haystack) must be of type array".into()),
    };

    let strict = if args.len() == 3 {
        vm.arena.get(args[2]).value.to_bool()
    } else {
        false
    };

    for (_, value_handle) in haystack.map.iter() {
        let candidate = vm.arena.get(*value_handle).value.clone();
        if values_equal(&needle, &candidate, strict) {
            return Ok(vm.arena.alloc(Val::Bool(true)));
        }
    }

    Ok(vm.arena.alloc(Val::Bool(false)))
}

fn values_equal(a: &Val, b: &Val, strict: bool) -> bool {
    if strict {
        return a == b;
    }

    match (a, b) {
        (Val::Bool(_), _) | (_, Val::Bool(_)) => a.to_bool() == b.to_bool(),
        (Val::Int(_), Val::Int(_)) => a == b,
        (Val::Float(_), Val::Float(_)) => a == b,
        (Val::Int(_), Val::Float(_)) | (Val::Float(_), Val::Int(_)) => a.to_float() == b.to_float(),
        (Val::String(_), Val::String(_)) => a == b,
        (Val::String(_), Val::Int(_))
        | (Val::Int(_), Val::String(_))
        | (Val::String(_), Val::Float(_))
        | (Val::Float(_), Val::String(_)) => a.to_float() == b.to_float(),
        _ => a == b,
    }
}

pub fn php_ksort(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("ksort() expects at least 1 parameter".into());
    }

    let arr_handle = args[0];
    let arr_slot = vm.arena.get(arr_handle);

    if let Val::Array(arr_rc) = &arr_slot.value {
        let mut arr_data = (**arr_rc).clone();

        // Sort keys: collect entries, sort, and rebuild
        let mut entries: Vec<_> = arr_data.map.iter().map(|(k, v)| (k.clone(), *v)).collect();
        entries.sort_by(|(a, _), (b, _)| match (a, b) {
            (ArrayKey::Int(i1), ArrayKey::Int(i2)) => i1.cmp(i2),
            (ArrayKey::Str(s1), ArrayKey::Str(s2)) => s1.cmp(s2),
            (ArrayKey::Int(_), ArrayKey::Str(_)) => std::cmp::Ordering::Less,
            (ArrayKey::Str(_), ArrayKey::Int(_)) => std::cmp::Ordering::Greater,
        });

        let sorted_map: IndexMap<_, _> = entries.into_iter().collect();
        arr_data.map = sorted_map;

        let slot = vm.arena.get_mut(arr_handle);
        slot.value = Val::Array(std::rc::Rc::new(arr_data));

        Ok(vm.arena.alloc(Val::Bool(true)))
    } else {
        Err("ksort() expects parameter 1 to be array".into())
    }
}

pub fn php_array_push(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("array_push() expects at least 1 parameter".into());
    }

    let arr_handle = args[0];
    let arr_val = vm.arena.get(arr_handle);

    if let Val::Array(arr_rc) = &arr_val.value {
        let mut arr_data = (**arr_rc).clone();
        for &arg in &args[1..] {
            arr_data.push(arg);
        }
        let new_len = arr_data.map.len() as i64;
        let slot = vm.arena.get_mut(arr_handle);
        slot.value = Val::Array(std::rc::Rc::new(arr_data));
        Ok(vm.arena.alloc(Val::Int(new_len)))
    } else {
        Err("array_push() expects parameter 1 to be array".into())
    }
}

pub fn php_array_pop(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() != 1 {
        return Err("array_pop() expects exactly 1 parameter".into());
    }

    let arr_handle = args[0];
    let arr_val = vm.arena.get(arr_handle);

    if let Val::Array(arr_rc) = &arr_val.value {
        if arr_rc.map.is_empty() {
            return Ok(vm.arena.alloc(Val::Null));
        }

        let mut arr_data = (**arr_rc).clone();
        let popped_val = arr_data
            .map
            .pop()
            .map(|(_, v)| v)
            .unwrap_or_else(|| vm.arena.alloc(Val::Null));

        let slot = vm.arena.get_mut(arr_handle);
        slot.value = Val::Array(std::rc::Rc::new(arr_data));
        Ok(popped_val)
    } else {
        Err("array_pop() expects parameter 1 to be array".into())
    }
}

pub fn php_array_shift(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() != 1 {
        return Err("array_shift() expects exactly 1 parameter".into());
    }

    let arr_handle = args[0];
    let arr_val = vm.arena.get(arr_handle);

    if let Val::Array(arr_rc) = &arr_val.value {
        if arr_rc.map.is_empty() {
            return Ok(vm.arena.alloc(Val::Null));
        }

        let mut arr_data = (**arr_rc).clone();

        // Remove first element
        let shifted_val = if let Some((_, val)) = arr_data.map.shift_remove_index(0) {
            val
        } else {
            vm.arena.alloc(Val::Null)
        };

        // Re-index numeric keys
        let mut new_map = IndexMap::new();
        let mut next_int = 0;
        for (key, val) in arr_data.map {
            match key {
                ArrayKey::Int(_) => {
                    new_map.insert(ArrayKey::Int(next_int), val);
                    next_int += 1;
                }
                ArrayKey::Str(s) => {
                    new_map.insert(ArrayKey::Str(s), val);
                }
            }
        }
        arr_data.map = new_map;
        arr_data.next_free = next_int;

        let slot = vm.arena.get_mut(arr_handle);
        slot.value = Val::Array(std::rc::Rc::new(arr_data));
        Ok(shifted_val)
    } else {
        Err("array_shift() expects parameter 1 to be array".into())
    }
}

pub fn php_array_unshift(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("array_unshift() expects at least 1 parameter".into());
    }

    let arr_handle = args[0];
    let arr_val = vm.arena.get(arr_handle);

    if let Val::Array(arr_rc) = &arr_val.value {
        let mut arr_data = (**arr_rc).clone();

        // Rebuild array with new elements prepended and numeric keys re-indexed
        let mut new_map = IndexMap::new();
        let mut next_int = 0;

        // Add new elements first (from args[1..])
        for &arg in &args[1..] {
            new_map.insert(ArrayKey::Int(next_int), arg);
            next_int += 1;
        }

        // Then add existing elements, re-indexing numeric keys
        for (key, val_handle) in arr_data.map {
            match key {
                ArrayKey::Int(_) => {
                    new_map.insert(ArrayKey::Int(next_int), val_handle);
                    next_int += 1;
                }
                ArrayKey::Str(s) => {
                    new_map.insert(ArrayKey::Str(s), val_handle);
                }
            }
        }

        arr_data.map = new_map;
        arr_data.next_free = next_int;
        let new_len = arr_data.map.len() as i64;

        let slot = vm.arena.get_mut(arr_handle);
        slot.value = Val::Array(std::rc::Rc::new(arr_data));

        Ok(vm.arena.alloc(Val::Int(new_len)))
    } else {
        Err("array_unshift() expects parameter 1 to be array".into())
    }
}

pub fn php_current(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("current() expects exactly 1 parameter".into());
    }

    let arr_handle = args[0];
    let arr_val = vm.arena.get(arr_handle);

    if let Val::Array(arr_rc) = &arr_val.value {
        if let Some((_, val_handle)) = arr_rc.map.get_index(arr_rc.internal_ptr) {
            Ok(*val_handle)
        } else {
            Ok(vm.arena.alloc(Val::Bool(false)))
        }
    } else {
        Ok(vm.arena.alloc(Val::Bool(false)))
    }
}

pub fn php_next(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("next() expects exactly 1 parameter".into());
    }

    let arr_handle = args[0];
    let arr_val = vm.arena.get(arr_handle);

    if let Val::Array(arr_rc) = &arr_val.value {
        let mut arr_data = (**arr_rc).clone();
        arr_data.internal_ptr += 1;

        let result = if let Some((_, val_handle)) = arr_data.map.get_index(arr_data.internal_ptr) {
            *val_handle
        } else {
            vm.arena.alloc(Val::Bool(false))
        };

        let slot = vm.arena.get_mut(arr_handle);
        slot.value = Val::Array(std::rc::Rc::new(arr_data));
        Ok(result)
    } else {
        Ok(vm.arena.alloc(Val::Bool(false)))
    }
}

pub fn php_prev(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("prev() expects exactly 1 parameter".into());
    }

    let arr_handle = args[0];
    let arr_val = vm.arena.get(arr_handle);

    if let Val::Array(arr_rc) = &arr_val.value {
        let mut arr_data = (**arr_rc).clone();
        if arr_data.internal_ptr > 0 {
            arr_data.internal_ptr -= 1;
            if let Some((_, val_handle)) = arr_data.map.get_index(arr_data.internal_ptr) {
                let res = *val_handle;
                let slot = vm.arena.get_mut(arr_handle);
                slot.value = Val::Array(std::rc::Rc::new(arr_data));
                return Ok(res);
            }
        }
        // If it was already at 0 or becomes invalid
        arr_data.internal_ptr = arr_data.map.len(); // Move to end+1
        let slot = vm.arena.get_mut(arr_handle);
        slot.value = Val::Array(std::rc::Rc::new(arr_data));
        Ok(vm.arena.alloc(Val::Bool(false)))
    } else {
        Ok(vm.arena.alloc(Val::Bool(false)))
    }
}

pub fn php_reset(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("reset() expects exactly 1 parameter".into());
    }

    let arr_handle = args[0];
    let arr_val = vm.arena.get(arr_handle);

    if let Val::Array(arr_rc) = &arr_val.value {
        let mut arr_data = (**arr_rc).clone();
        arr_data.internal_ptr = 0;

        let result = if let Some((_, val_handle)) = arr_data.map.get_index(0) {
            *val_handle
        } else {
            vm.arena.alloc(Val::Bool(false))
        };

        let slot = vm.arena.get_mut(arr_handle);
        slot.value = Val::Array(std::rc::Rc::new(arr_data));
        Ok(result)
    } else {
        Ok(vm.arena.alloc(Val::Bool(false)))
    }
}

pub fn php_end(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("end() expects exactly 1 parameter".into());
    }

    let arr_handle = args[0];
    let arr_val = vm.arena.get(arr_handle);

    if let Val::Array(arr_rc) = &arr_val.value {
        let mut arr_data = (**arr_rc).clone();
        let len = arr_data.map.len();
        if len > 0 {
            arr_data.internal_ptr = len - 1;
            let result = *arr_data.map.get_index(len - 1).unwrap().1;
            let slot = vm.arena.get_mut(arr_handle);
            slot.value = Val::Array(std::rc::Rc::new(arr_data));
            Ok(result)
        } else {
            arr_data.internal_ptr = 0;
            let slot = vm.arena.get_mut(arr_handle);
            slot.value = Val::Array(std::rc::Rc::new(arr_data));
            Ok(vm.arena.alloc(Val::Bool(false)))
        }
    } else {
        Ok(vm.arena.alloc(Val::Bool(false)))
    }
}

pub fn php_key(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("key() expects exactly 1 parameter".into());
    }

    let arr_handle = args[0];
    let arr_val = vm.arena.get(arr_handle);

    if let Val::Array(arr_rc) = &arr_val.value {
        if let Some((key, _)) = arr_rc.map.get_index(arr_rc.internal_ptr) {
            let key_val = match key {
                ArrayKey::Int(i) => Val::Int(*i),
                ArrayKey::Str(s) => Val::String((*s).clone().into()),
            };
            Ok(vm.arena.alloc(key_val))
        } else {
            Ok(vm.arena.alloc(Val::Null))
        }
    } else {
        Ok(vm.arena.alloc(Val::Null))
    }
}

pub fn php_array_key_exists(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() != 2 {
        return Err("array_key_exists() expects exactly 2 parameters".into());
    }

    let key_val = vm.arena.get(args[0]).value.clone();
    let arr_val = vm.arena.get(args[1]);

    if let Val::Array(arr_rc) = &arr_val.value {
        let key = match key_val {
            Val::Int(i) => ArrayKey::Int(i),
            Val::String(s) => ArrayKey::Str(s.into()),
            Val::Float(f) => ArrayKey::Int(f as i64),
            Val::Bool(b) => ArrayKey::Int(if b { 1 } else { 0 }),
            Val::Null => ArrayKey::Str(vec![].into()),
            _ => {
                return Err(
                    "array_key_exists(): Argument #1 ($key) must be a valid array key".into(),
                );
            }
        };

        let exists = arr_rc.map.contains_key(&key);
        Ok(vm.arena.alloc(Val::Bool(exists)))
    } else {
        Err("array_key_exists(): Argument #2 ($array) must be of type array".into())
    }
}

pub fn php_array_fill(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() != 3 {
        return Err("array_fill() expects exactly 3 parameters".into());
    }

    let start_index = match vm.arena.get(args[0]).value {
        Val::Int(i) => i,
        _ => return Err("array_fill(): Argument #1 ($start_index) must be of type int".into()),
    };

    let count = match vm.arena.get(args[1]).value {
        Val::Int(i) => i,
        _ => return Err("array_fill(): Argument #2 ($count) must be of type int".into()),
    };

    if count < 0 {
        return Err("array_fill(): Argument #2 ($count) must be greater than or equal to 0".into());
    }

    let value = args[2];
    let mut map = IndexMap::new();
    let mut next_free = 0;

    if count > 0 {
        for i in 0..count {
            let key = if i == 0 {
                start_index
            } else {
                // If start_index is negative, the next key is 0, then 1, 2...
                // Wait, PHP behavior:
                // array_fill(-5, 3, 'a') -> [-5 => 'a', 0 => 'a', 1 => 'a']
                if start_index < 0 {
                    i - 1
                } else {
                    start_index + i
                }
            };
            map.insert(ArrayKey::Int(key), value);
            if key >= next_free {
                next_free = key + 1;
            }
        }
    }

    Ok(vm.arena.alloc(Val::Array(
        crate::core::value::ArrayData {
            map,
            next_free,
            internal_ptr: 0,
        }
        .into(),
    )))
}

pub fn php_array_fill_keys(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() != 2 {
        return Err("array_fill_keys() expects exactly 2 parameters".into());
    }

    let keys_val = vm.arena.get(args[0]);
    let keys_arr = match &keys_val.value {
        Val::Array(arr) => arr,
        _ => {
            return Err("array_fill_keys(): Argument #1 ($keys) must be of type array".into());
        }
    };

    let value = args[1];
    let mut map = IndexMap::new();
    let mut next_free = 0;

    for (_, &key_handle) in &keys_arr.map {
        let key_val = vm.arena.get(key_handle).value.clone();
        let key = match key_val {
            Val::Int(i) => ArrayKey::Int(i),
            Val::String(s) => ArrayKey::Str(s.into()),
            Val::Float(f) => ArrayKey::Int(f as i64),
            Val::Bool(b) => ArrayKey::Int(if b { 1 } else { 0 }),
            Val::Null => ArrayKey::Str(vec![].into()),
            _ => {
                // PHP allows objects that can be cast to string? No, array_fill_keys expects scalar keys.
                return Err("array_fill_keys(): Keys must be int or string".into());
            }
        };

        if let ArrayKey::Int(i) = key {
            if i >= next_free {
                next_free = i + 1;
            }
        }
        map.insert(key, value);
    }

    Ok(vm.arena.alloc(Val::Array(
        crate::core::value::ArrayData {
            map,
            next_free,
            internal_ptr: 0,
        }
        .into(),
    )))
}

pub fn php_range(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() < 2 || args.len() > 3 {
        return Err("range() expects 2 or 3 parameters".into());
    }

    let start_val = vm.arena.get(args[0]).value.clone();
    let end_val = vm.arena.get(args[1]).value.clone();
    let step_val = if args.len() == 3 {
        vm.arena.get(args[2]).value.clone()
    } else {
        Val::Int(1)
    };

    // Check if we are dealing with strings (characters)
    if let (Val::String(s), Val::String(e)) = (&start_val, &end_val) {
        if s.len() == 1 && e.len() == 1 {
            let start_char = s[0];
            let end_char = e[0];
            let step = step_val.to_int() as i64;
            if step <= 0 {
                return Err("range(): step must be positive".into());
            }

            let mut map = IndexMap::new();
            let mut idx = 0;
            if start_char <= end_char {
                let mut curr = start_char;
                while curr <= end_char {
                    map.insert(
                        ArrayKey::Int(idx),
                        vm.arena.alloc(Val::String(vec![curr].into())),
                    );
                    idx += 1;
                    if curr as u64 + step as u64 > 255 {
                        break;
                    }
                    curr = (curr as u64 + step as u64) as u8;
                }
            } else {
                let mut curr = start_char;
                while curr >= end_char {
                    map.insert(
                        ArrayKey::Int(idx),
                        vm.arena.alloc(Val::String(vec![curr].into())),
                    );
                    idx += 1;
                    if (curr as i64) - step < 0 {
                        break;
                    }
                    curr = (curr as i64 - step) as u8;
                }
            }
            return Ok(vm.arena.alloc(Val::Array(
                crate::core::value::ArrayData {
                    map,
                    next_free: idx,
                    internal_ptr: 0,
                }
                .into(),
            )));
        }
    }

    // Numeric range
    let start = start_val.to_float();
    let end = end_val.to_float();
    let step = step_val.to_float();

    if step == 0.0 {
        return Err("range(): step cannot be 0".into());
    }

    let mut map = IndexMap::new();
    let mut idx = 0;

    let is_float = matches!(start_val, Val::Float(_))
        || matches!(end_val, Val::Float(_))
        || (args.len() == 3 && matches!(step_val, Val::Float(_)))
        || step.fract() != 0.0;

    if start <= end {
        if step < 0.0 {
            return Err("range(): step must be positive".into());
        }
        let mut curr = start;
        while curr <= end + 0.000000000001 {
            // Small epsilon for float comparison
            let val = if is_float {
                Val::Float(curr)
            } else {
                Val::Int(curr.round() as i64)
            };
            map.insert(ArrayKey::Int(idx), vm.arena.alloc(val));
            idx += 1;
            curr += step;
        }
    } else {
        let abs_step = step.abs();
        let mut curr = start;
        while curr >= end - 0.000000000001 {
            let val = if is_float {
                Val::Float(curr)
            } else {
                Val::Int(curr.round() as i64)
            };
            map.insert(ArrayKey::Int(idx), vm.arena.alloc(val));
            idx += 1;
            curr -= abs_step;
        }
    }

    Ok(vm.arena.alloc(Val::Array(
        crate::core::value::ArrayData {
            map,
            next_free: idx,
            internal_ptr: 0,
        }
        .into(),
    )))
}

pub fn php_array_key_first(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() != 1 {
        return Err("array_key_first() expects exactly 1 parameter".into());
    }

    let val = vm.arena.get(args[0]);
    if let Val::Array(arr) = &val.value {
        if let Some((key, _)) = arr.map.get_index(0) {
            let key_val = match key {
                ArrayKey::Int(i) => Val::Int(*i),
                ArrayKey::Str(s) => Val::String((*s).clone().into()),
            };
            return Ok(vm.arena.alloc(key_val));
        }
    }
    Ok(vm.arena.alloc(Val::Null))
}

pub fn php_array_key_last(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() != 1 {
        return Err("array_key_last() expects exactly 1 parameter".into());
    }

    let val = vm.arena.get(args[0]);
    if let Val::Array(arr) = &val.value {
        let len = arr.map.len();
        if len > 0 {
            if let Some((key, _)) = arr.map.get_index(len - 1) {
                let key_val = match key {
                    ArrayKey::Int(i) => Val::Int(*i),
                    ArrayKey::Str(s) => Val::String((*s).clone().into()),
                };
                return Ok(vm.arena.alloc(key_val));
            }
        }
    }
    Ok(vm.arena.alloc(Val::Null))
}

pub fn php_array_first(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() != 1 {
        return Err("array_first() expects exactly 1 parameter".into());
    }

    let val = vm.arena.get(args[0]);
    if let Val::Array(arr) = &val.value {
        if let Some((_, val_handle)) = arr.map.get_index(0) {
            return Ok(*val_handle);
        }
    }
    Ok(vm.arena.alloc(Val::Null))
}

pub fn php_array_last(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() != 1 {
        return Err("array_last() expects exactly 1 parameter".into());
    }

    let val = vm.arena.get(args[0]);
    if let Val::Array(arr) = &val.value {
        let len = arr.map.len();
        if len > 0 {
            if let Some((_, val_handle)) = arr.map.get_index(len - 1) {
                return Ok(*val_handle);
            }
        }
    }
    Ok(vm.arena.alloc(Val::Null))
}

pub fn php_array_flip(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() != 1 {
        return Err("array_flip() expects exactly 1 parameter".into());
    }

    let arr = {
        let val = vm.arena.get(args[0]);
        match &val.value {
            Val::Array(arr) => arr.clone(),
            _ => return Err("array_flip(): Argument #1 ($array) must be of type array".into()),
        }
    };

    let mut map = IndexMap::new();
    let mut next_free = 0;

    for (key, &val_handle) in &arr.map {
        let val_val = vm.arena.get(val_handle).value.clone();
        let new_key = match val_val {
            Val::Int(i) => ArrayKey::Int(i),
            Val::String(s) => ArrayKey::Str(s.into()),
            Val::Float(f) => ArrayKey::Int(f as i64),
            Val::Bool(b) => ArrayKey::Int(if b { 1 } else { 0 }),
            Val::Null => ArrayKey::Str(vec![].into()),
            _ => {
                // PHP warns and skips non-scalar values
                continue;
            }
        };

        let new_val = match key {
            ArrayKey::Int(i) => Val::Int(*i),
            ArrayKey::Str(s) => Val::String((*s).clone().into()),
        };

        if let ArrayKey::Int(i) = new_key {
            if i >= next_free {
                next_free = i + 1;
            }
        }
        map.insert(new_key, vm.arena.alloc(new_val));
    }

    Ok(vm.arena.alloc(Val::Array(
        crate::core::value::ArrayData {
            map,
            next_free,
            internal_ptr: 0,
        }
        .into(),
    )))
}

pub fn php_array_reverse(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() < 1 || args.len() > 2 {
        return Err("array_reverse() expects 1 or 2 parameters".into());
    }

    let val = vm.arena.get(args[0]);
    let arr = match &val.value {
        Val::Array(arr) => arr,
        _ => return Err("array_reverse(): Argument #1 ($array) must be of type array".into()),
    };

    let preserve_keys = if args.len() == 2 {
        vm.arena.get(args[1]).value.to_bool()
    } else {
        false
    };

    let mut map = IndexMap::new();
    let mut next_free = 0;

    for (key, &val_handle) in arr.map.iter().rev() {
        let new_key = if preserve_keys {
            key.clone()
        } else {
            match key {
                ArrayKey::Int(_) => {
                    let k = ArrayKey::Int(next_free);
                    next_free += 1;
                    k
                }
                ArrayKey::Str(s) => ArrayKey::Str(s.clone()),
            }
        };

        if let ArrayKey::Int(i) = &new_key {
            if *i >= next_free {
                next_free = *i + 1;
            }
        }
        map.insert(new_key, val_handle);
    }

    Ok(vm.arena.alloc(Val::Array(
        crate::core::value::ArrayData {
            map,
            next_free,
            internal_ptr: 0,
        }
        .into(),
    )))
}

pub fn php_array_column(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() < 2 || args.len() > 3 {
        return Err("array_column() expects 2 or 3 parameters".into());
    }

    let input_val = vm.arena.get(args[0]);
    let input_arr = match &input_val.value {
        Val::Array(arr) => arr,
        _ => return Err("array_column(): Argument #1 ($input) must be of type array".into()),
    };

    let column_key_handle = args[1];
    let column_key_val = vm.arena.get(column_key_handle).value.clone();
    let column_key = match column_key_val {
        Val::Null => None,
        Val::Int(i) => Some(ArrayKey::Int(i)),
        Val::String(s) => Some(ArrayKey::Str(s.into())),
        _ => return Err("array_column(): Column key must be int, string or null".into()),
    };

    let index_key = if args.len() == 3 {
        let index_key_val = vm.arena.get(args[2]).value.clone();
        match index_key_val {
            Val::Null => None,
            Val::Int(i) => Some(ArrayKey::Int(i)),
            Val::String(s) => Some(ArrayKey::Str(s.into())),
            _ => return Err("array_column(): Index key must be int, string or null".into()),
        }
    } else {
        None
    };

    let mut map = IndexMap::new();
    let mut next_free = 0;

    for (_, &row_handle) in &input_arr.map {
        let row_val = vm.arena.get(row_handle);
        let row_arr = match &row_val.value {
            Val::Array(arr) => Some(arr),
            _ => None, // Could also be an object, but let's stick to arrays for now
        };

        if let Some(arr) = row_arr {
            let value_to_insert = if let Some(ref ck) = column_key {
                if let Some(&vh) = arr.map.get(ck) {
                    vh
                } else {
                    continue;
                }
            } else {
                row_handle
            };

            let key_to_use = if let Some(ref ik) = index_key {
                if let Some(&kh) = arr.map.get(ik) {
                    let kv = vm.arena.get(kh).value.clone();
                    match kv {
                        Val::Int(i) => ArrayKey::Int(i),
                        Val::String(s) => ArrayKey::Str(s.into()),
                        Val::Float(f) => ArrayKey::Int(f as i64),
                        Val::Bool(b) => ArrayKey::Int(if b { 1 } else { 0 }),
                        Val::Null => ArrayKey::Str(vec![].into()),
                        _ => {
                            let k = ArrayKey::Int(next_free);
                            next_free += 1;
                            k
                        }
                    }
                } else {
                    let k = ArrayKey::Int(next_free);
                    next_free += 1;
                    k
                }
            } else {
                let k = ArrayKey::Int(next_free);
                next_free += 1;
                k
            };

            if let ArrayKey::Int(i) = key_to_use {
                if i >= next_free {
                    next_free = i + 1;
                }
            }
            map.insert(key_to_use, value_to_insert);
        }
    }

    Ok(vm.arena.alloc(Val::Array(
        crate::core::value::ArrayData {
            map,
            next_free,
            internal_ptr: 0,
        }
        .into(),
    )))
}

pub fn php_array_chunk(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() < 2 || args.len() > 3 {
        return Err("array_chunk() expects 2 or 3 parameters".into());
    }

    let arr = {
        let val = vm.arena.get(args[0]);
        match &val.value {
            Val::Array(arr) => arr.clone(),
            _ => return Err("array_chunk(): Argument #1 ($array) must be of type array".into()),
        }
    };

    let length = match vm.arena.get(args[1]).value {
        Val::Int(i) => i,
        _ => return Err("array_chunk(): Argument #2 ($length) must be of type int".into()),
    };

    if length <= 0 {
        return Err("array_chunk(): Argument #2 ($length) must be greater than 0".into());
    }

    let preserve_keys = if args.len() == 3 {
        vm.arena.get(args[2]).value.to_bool()
    } else {
        false
    };

    let mut result_map = IndexMap::new();
    let mut chunk_idx = 0;

    let mut current_chunk_map = IndexMap::new();
    let mut current_chunk_next_free = 0;

    for (key, &val_handle) in &arr.map {
        let chunk_key = if preserve_keys {
            key.clone()
        } else {
            let k = ArrayKey::Int(current_chunk_next_free);
            current_chunk_next_free += 1;
            k
        };

        if let ArrayKey::Int(i) = &chunk_key {
            if *i >= current_chunk_next_free {
                current_chunk_next_free = *i + 1;
            }
        }
        current_chunk_map.insert(chunk_key, val_handle);

        if current_chunk_map.len() == length as usize {
            let chunk_arr = Val::Array(
                crate::core::value::ArrayData {
                    map: current_chunk_map,
                    next_free: current_chunk_next_free,
                    internal_ptr: 0,
                }
                .into(),
            );
            result_map.insert(ArrayKey::Int(chunk_idx), vm.arena.alloc(chunk_arr));
            chunk_idx += 1;
            current_chunk_map = IndexMap::new();
            current_chunk_next_free = 0;
        }
    }

    if !current_chunk_map.is_empty() {
        let chunk_arr = Val::Array(
            crate::core::value::ArrayData {
                map: current_chunk_map,
                next_free: current_chunk_next_free,
                internal_ptr: 0,
            }
            .into(),
        );
        result_map.insert(ArrayKey::Int(chunk_idx), vm.arena.alloc(chunk_arr));
        chunk_idx += 1;
    }

    Ok(vm.arena.alloc(Val::Array(
        crate::core::value::ArrayData {
            map: result_map,
            next_free: chunk_idx,
            internal_ptr: 0,
        }
        .into(),
    )))
}

pub fn php_array_combine(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() != 2 {
        return Err("array_combine() expects exactly 2 parameters".into());
    }

    let keys_val = vm.arena.get(args[0]);
    let values_val = vm.arena.get(args[1]);

    let keys_arr = match &keys_val.value {
        Val::Array(arr) => arr,
        _ => return Err("array_combine(): Argument #1 ($keys) must be of type array".into()),
    };

    let values_arr = match &values_val.value {
        Val::Array(arr) => arr,
        _ => return Err("array_combine(): Argument #2 ($values) must be of type array".into()),
    };

    if keys_arr.map.len() != values_arr.map.len() {
        return Err(
            "array_combine(): Both parameters should have an equal number of elements".into(),
        );
    }

    let mut map = IndexMap::new();
    let mut next_free = 0;

    for ((_, &key_handle), (_, &val_handle)) in keys_arr.map.iter().zip(values_arr.map.iter()) {
        let key_val = vm.arena.get(key_handle).value.clone();
        let key = match key_val {
            Val::Int(i) => ArrayKey::Int(i),
            Val::String(s) => ArrayKey::Str(s.into()),
            Val::Float(f) => ArrayKey::Int(f as i64),
            Val::Bool(b) => ArrayKey::Int(if b { 1 } else { 0 }),
            Val::Null => ArrayKey::Str(vec![].into()),
            _ => return Err("array_combine(): Keys must be scalar".into()),
        };

        if let ArrayKey::Int(i) = key {
            if i >= next_free {
                next_free = i + 1;
            }
        }
        map.insert(key, val_handle);
    }

    Ok(vm.arena.alloc(Val::Array(
        crate::core::value::ArrayData {
            map,
            next_free,
            internal_ptr: 0,
        }
        .into(),
    )))
}

pub fn php_array_count_values(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() != 1 {
        return Err("array_count_values() expects exactly 1 parameter".into());
    }

    let val = vm.arena.get(args[0]);
    let arr = match &val.value {
        Val::Array(arr) => arr,
        _ => return Err("array_count_values(): Argument #1 ($array) must be of type array".into()),
    };

    let mut counts: IndexMap<ArrayKey, i64> = IndexMap::new();

    for (_, &val_handle) in &arr.map {
        let v = vm.arena.get(val_handle).value.clone();
        let key = match v {
            Val::Int(i) => ArrayKey::Int(i),
            Val::String(s) => ArrayKey::Str(s.into()),
            _ => continue, // PHP only counts strings and integers
        };

        *counts.entry(key).or_insert(0) += 1;
    }

    let mut map = IndexMap::new();
    let mut next_free = 0;

    for (key, count) in counts {
        if let ArrayKey::Int(i) = key {
            if i >= next_free {
                next_free = i + 1;
            }
        }
        map.insert(key, vm.arena.alloc(Val::Int(count)));
    }

    Ok(vm.arena.alloc(Val::Array(
        crate::core::value::ArrayData {
            map,
            next_free,
            internal_ptr: 0,
        }
        .into(),
    )))
}

pub fn php_array_slice(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() < 2 || args.len() > 4 {
        return Err("array_slice() expects between 2 and 4 parameters".into());
    }

    let val = vm.arena.get(args[0]);
    let arr = match &val.value {
        Val::Array(arr) => arr,
        _ => return Err("array_slice(): Argument #1 ($array) must be of type array".into()),
    };

    let offset = match vm.arena.get(args[1]).value {
        Val::Int(i) => i,
        _ => return Err("array_slice(): Argument #2 ($offset) must be of type int".into()),
    };

    let length = if args.len() >= 3 {
        match vm.arena.get(args[2]).value {
            Val::Int(i) => Some(i),
            Val::Null => None,
            _ => {
                return Err(
                    "array_slice(): Argument #3 ($length) must be of type int or null".into(),
                );
            }
        }
    } else {
        None
    };

    let preserve_keys = if args.len() == 4 {
        vm.arena.get(args[3]).value.to_bool()
    } else {
        false
    };

    let total_len = arr.map.len() as i64;
    let start = if offset >= 0 {
        offset
    } else {
        total_len + offset
    };
    let start = start.max(0).min(total_len) as usize;

    let end = if let Some(l) = length {
        if l >= 0 {
            (start as i64 + l).min(total_len)
        } else {
            (total_len + l).max(start as i64)
        }
    } else {
        total_len
    } as usize;

    let mut map = IndexMap::new();
    let mut next_free = 0;

    for i in start..end {
        if let Some((key, &val_handle)) = arr.map.get_index(i) {
            let new_key = if preserve_keys {
                key.clone()
            } else {
                match key {
                    ArrayKey::Int(_) => {
                        let k = ArrayKey::Int(next_free);
                        next_free += 1;
                        k
                    }
                    ArrayKey::Str(s) => ArrayKey::Str(s.clone()),
                }
            };

            if let ArrayKey::Int(i) = &new_key {
                if *i >= next_free {
                    next_free = *i + 1;
                }
            }
            map.insert(new_key, val_handle);
        }
    }

    Ok(vm.arena.alloc(Val::Array(
        crate::core::value::ArrayData {
            map,
            next_free,
            internal_ptr: 0,
        }
        .into(),
    )))
}

pub fn php_array_pad(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() != 3 {
        return Err("array_pad() expects exactly 3 parameters".into());
    }

    let val = vm.arena.get(args[0]);
    let arr = match &val.value {
        Val::Array(arr) => arr,
        _ => return Err("array_pad(): Argument #1 ($array) must be of type array".into()),
    };

    let length = match vm.arena.get(args[1]).value {
        Val::Int(i) => i,
        _ => return Err("array_pad(): Argument #2 ($length) must be of type int".into()),
    };

    let value = args[2];

    let current_len = arr.map.len() as i64;
    let abs_length = length.abs();

    if abs_length <= current_len {
        return Ok(args[0]);
    }

    let mut map = IndexMap::new();
    let mut next_free = 0;

    if length > 0 {
        // Pad at the end
        for (key, &val_handle) in &arr.map {
            if let ArrayKey::Int(i) = key {
                if *i >= next_free {
                    next_free = *i + 1;
                }
            }
            map.insert(key.clone(), val_handle);
        }
        while (map.len() as i64) < length {
            map.insert(ArrayKey::Int(next_free), value);
            next_free += 1;
        }
    } else {
        // Pad at the beginning
        let pad_count = abs_length - current_len;
        for i in 0..pad_count {
            map.insert(ArrayKey::Int(i), value);
            next_free = i + 1;
        }
        for (key, &val_handle) in &arr.map {
            let new_key = match key {
                ArrayKey::Int(i) => {
                    let k = ArrayKey::Int(i + pad_count);
                    if i + pad_count >= next_free {
                        next_free = i + pad_count + 1;
                    }
                    k
                }
                ArrayKey::Str(s) => ArrayKey::Str(s.clone()),
            };
            map.insert(new_key, val_handle);
        }
    }

    Ok(vm.arena.alloc(Val::Array(
        crate::core::value::ArrayData {
            map,
            next_free,
            internal_ptr: 0,
        }
        .into(),
    )))
}

pub fn php_array_sum(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() != 1 {
        return Err("array_sum() expects exactly 1 parameter".into());
    }

    let val = vm.arena.get(args[0]);
    let arr = match &val.value {
        Val::Array(arr) => arr,
        _ => return Err("array_sum(): Argument #1 ($array) must be of type array".into()),
    };

    let mut sum_f = 0.0;
    let mut is_float = false;

    for (_, &val_handle) in &arr.map {
        let v = vm.arena.get(val_handle).value.clone();
        match &v {
            Val::Int(i) => sum_f += *i as f64,
            Val::Float(f) => {
                sum_f += *f;
                is_float = true;
            }
            Val::String(s) => {
                let (i, f) = Val::parse_numeric_string(s);
                if f {
                    sum_f += v.to_float();
                    is_float = true;
                } else {
                    sum_f += i as f64;
                }
            }
            _ => sum_f += v.to_float(),
        }
    }

    if is_float {
        Ok(vm.arena.alloc(Val::Float(sum_f)))
    } else {
        Ok(vm.arena.alloc(Val::Int(sum_f as i64)))
    }
}

pub fn php_array_product(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() != 1 {
        return Err("array_product() expects exactly 1 parameter".into());
    }

    let val = vm.arena.get(args[0]);
    let arr = match &val.value {
        Val::Array(arr) => arr,
        _ => return Err("array_product(): Argument #1 ($array) must be of type array".into()),
    };

    if arr.map.is_empty() {
        return Ok(vm.arena.alloc(Val::Int(1)));
    }

    let mut prod_f = 1.0;
    let mut is_float = false;

    for (_, &val_handle) in &arr.map {
        let v = vm.arena.get(val_handle).value.clone();
        match v {
            Val::Int(i) => prod_f *= i as f64,
            Val::Float(f) => {
                prod_f *= f;
                is_float = true;
            }
            _ => {
                prod_f *= v.to_float();
                if matches!(v, Val::Float(_)) {
                    is_float = true;
                }
            }
        }
    }

    if is_float {
        Ok(vm.arena.alloc(Val::Float(prod_f)))
    } else {
        Ok(vm.arena.alloc(Val::Int(prod_f as i64)))
    }
}

pub fn php_array_reduce(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() < 2 || args.len() > 3 {
        return Err("array_reduce() expects 2 or 3 parameters".into());
    }

    let arr = {
        let val = vm.arena.get(args[0]);
        match &val.value {
            Val::Array(arr) => arr.clone(),
            _ => return Err("array_reduce(): Argument #1 ($array) must be of type array".into()),
        }
    };

    let callback = args[1];
    let mut carry = if args.len() == 3 {
        args[2]
    } else {
        vm.arena.alloc(Val::Null)
    };

    for (_, &val_handle) in &arr.map {
        carry = vm
            .call_callable(callback, smallvec![carry, val_handle])
            .map_err(|e| e.to_string())?;
    }

    Ok(carry)
}

pub fn php_array_map(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() < 2 {
        return Err("array_map() expects at least 2 parameters".into());
    }

    let callback = args[0];
    let is_callback_null = matches!(vm.arena.get(callback).value, Val::Null);

    let mut arrays = Vec::new();
    let mut max_len = 0;

    for &arg in &args[1..] {
        let val = vm.arena.get(arg).value.clone();
        match val {
            Val::Array(arr) => {
                arrays.push(arr.clone());
                max_len = max_len.max(arr.map.len());
            }
            Val::ConstArray(const_arr) => {
                let mut map = IndexMap::new();
                let entries: Vec<_> = const_arr.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                for (key, val) in entries {
                    let runtime_key = match key {
                        ConstArrayKey::Int(i) => ArrayKey::Int(i),
                        ConstArrayKey::Str(s) => ArrayKey::Str(s),
                    };
                    let handle = vm.arena.alloc(val);
                    map.insert(runtime_key, handle);
                }
                let arr = Rc::new(ArrayData::from(map));
                max_len = max_len.max(arr.map.len());
                arrays.push(arr);
            }
            _ => {
                return Err("array_map(): All arguments after the callback must be arrays".into());
            }
        }
    }

    let mut result_map = IndexMap::new();
    let mut next_free = 0;

    for i in 0..max_len {
        let mut callback_args = Vec::new();
        for arr in &arrays {
            if let Some((_, &val_handle)) = arr.map.get_index(i) {
                callback_args.push(val_handle);
            } else {
                callback_args.push(vm.arena.alloc(Val::Null));
            }
        }

        let result_val_handle = if is_callback_null {
            if arrays.len() == 1 {
                callback_args[0]
            } else {
                let mut inner_map = IndexMap::new();
                for (j, &arg_handle) in callback_args.iter().enumerate() {
                    inner_map.insert(ArrayKey::Int(j as i64), arg_handle);
                }
                vm.arena.alloc(Val::Array(
                    crate::core::value::ArrayData {
                        map: inner_map,
                        next_free: callback_args.len() as i64,
                        internal_ptr: 0,
                    }
                    .into(),
                ))
            }
        } else {
            vm.call_callable(callback, callback_args.into())
                .map_err(|e| e.to_string())?
        };

        result_map.insert(ArrayKey::Int(next_free), result_val_handle);
        next_free += 1;
    }

    Ok(vm.arena.alloc(Val::Array(
        crate::core::value::ArrayData {
            map: result_map,
            next_free,
            internal_ptr: 0,
        }
        .into(),
    )))
}

pub fn php_array_filter(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() < 1 || args.len() > 3 {
        return Err("array_filter() expects between 1 and 3 parameters".into());
    }

    let arr = {
        let val = vm.arena.get(args[0]);
        match &val.value {
            Val::Array(arr) => arr.clone(),
            _ => return Err("array_filter(): Argument #1 ($array) must be of type array".into()),
        }
    };

    let callback = if args.len() >= 2 {
        let c = args[1];
        if matches!(vm.arena.get(c).value, Val::Null) {
            None
        } else {
            Some(c)
        }
    } else {
        None
    };

    let mode = if args.len() == 3 {
        match vm.arena.get(args[2]).value {
            Val::Int(i) => i,
            _ => return Err("array_filter(): Argument #3 ($mode) must be of type int".into()),
        }
    } else {
        0
    };

    let mut result_map = IndexMap::new();
    let mut next_free = 0;

    for (key, &val_handle) in &arr.map {
        let keep = if let Some(cb) = callback {
            let cb_args = match mode {
                1 => smallvec![
                    val_handle,
                    vm.arena.alloc(match key {
                        ArrayKey::Int(i) => Val::Int(*i),
                        ArrayKey::Str(s) => Val::String((*s).clone().into()),
                    })
                ], // ARRAY_FILTER_USE_BOTH
                2 => smallvec![vm.arena.alloc(match key {
                    ArrayKey::Int(i) => Val::Int(*i),
                    ArrayKey::Str(s) => Val::String((*s).clone().into()),
                })], // ARRAY_FILTER_USE_KEY
                _ => smallvec![val_handle],
            };
            let res = vm.call_callable(cb, cb_args).map_err(|e| e.to_string())?;
            vm.arena.get(res).value.to_bool()
        } else {
            vm.arena.get(val_handle).value.to_bool()
        };

        if keep {
            if let ArrayKey::Int(i) = key {
                if *i >= next_free {
                    next_free = *i + 1;
                }
            }
            result_map.insert(key.clone(), val_handle);
        }
    }

    Ok(vm.arena.alloc(Val::Array(
        crate::core::value::ArrayData {
            map: result_map,
            next_free,
            internal_ptr: 0,
        }
        .into(),
    )))
}

pub fn php_array_walk(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() < 2 || args.len() > 3 {
        return Err("array_walk() expects 2 or 3 parameters".into());
    }

    let arr_handle = args[0];
    let callback = args[1];
    let userdata = if args.len() == 3 { Some(args[2]) } else { None };

    let arr_rc = {
        let arr_val = vm.arena.get(arr_handle);
        if let Val::Array(arr_rc) = &arr_val.value {
            arr_rc.clone()
        } else {
            return Err("array_walk() expects parameter 1 to be array".into());
        }
    };

    for (key, &val_handle) in &arr_rc.map {
        let key_handle = vm.arena.alloc(match key {
            ArrayKey::Int(i) => Val::Int(*i),
            ArrayKey::Str(s) => Val::String((*s).clone().into()),
        });
        let mut cb_args = smallvec![val_handle, key_handle];
        if let Some(ud) = userdata {
            cb_args.push(ud);
        }
        vm.call_callable(callback, cb_args)
            .map_err(|e| e.to_string())?;
    }
    Ok(vm.arena.alloc(Val::Bool(true)))
}

pub fn php_array_all(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() != 2 {
        return Err("array_all() expects exactly 2 parameters".into());
    }

    let arr = {
        let val = vm.arena.get(args[0]);
        match &val.value {
            Val::Array(arr) => arr.clone(),
            _ => return Err("array_all(): Argument #1 ($array) must be of type array".into()),
        }
    };

    let callback = args[1];

    for (key, &val_handle) in &arr.map {
        let key_handle = vm.arena.alloc(match key {
            ArrayKey::Int(i) => Val::Int(*i),
            ArrayKey::Str(s) => Val::String((*s).clone().into()),
        });
        let res = vm
            .call_callable(callback, smallvec![val_handle, key_handle])
            .map_err(|e| e.to_string())?;
        if !vm.arena.get(res).value.to_bool() {
            return Ok(vm.arena.alloc(Val::Bool(false)));
        }
    }

    Ok(vm.arena.alloc(Val::Bool(true)))
}

pub fn php_array_any(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() != 2 {
        return Err("array_any() expects exactly 2 parameters".into());
    }

    let arr = {
        let val = vm.arena.get(args[0]);
        match &val.value {
            Val::Array(arr) => arr.clone(),
            _ => return Err("array_any(): Argument #1 ($array) must be of type array".into()),
        }
    };

    let callback = args[1];

    for (key, &val_handle) in &arr.map {
        let key_handle = vm.arena.alloc(match key {
            ArrayKey::Int(i) => Val::Int(*i),
            ArrayKey::Str(s) => Val::String((*s).clone().into()),
        });
        let res = vm
            .call_callable(callback, smallvec![val_handle, key_handle])
            .map_err(|e| e.to_string())?;
        if vm.arena.get(res).value.to_bool() {
            return Ok(vm.arena.alloc(Val::Bool(true)));
        }
    }

    Ok(vm.arena.alloc(Val::Bool(false)))
}

pub fn php_array_find(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() != 2 {
        return Err("array_find() expects exactly 2 parameters".into());
    }

    let arr = {
        let val = vm.arena.get(args[0]);
        match &val.value {
            Val::Array(arr) => arr.clone(),
            _ => return Err("array_find(): Argument #1 ($array) must be of type array".into()),
        }
    };

    let callback = args[1];

    for (key, &val_handle) in &arr.map {
        let key_handle = vm.arena.alloc(match key {
            ArrayKey::Int(i) => Val::Int(*i),
            ArrayKey::Str(s) => Val::String((*s).clone().into()),
        });
        let res = vm
            .call_callable(callback, smallvec![val_handle, key_handle])
            .map_err(|e| e.to_string())?;
        if vm.arena.get(res).value.to_bool() {
            return Ok(val_handle);
        }
    }

    Ok(vm.arena.alloc(Val::Null))
}

pub fn php_array_find_key(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() != 2 {
        return Err("array_find_key() expects exactly 2 parameters".into());
    }

    let arr = {
        let val = vm.arena.get(args[0]);
        match &val.value {
            Val::Array(arr) => arr.clone(),
            _ => return Err("array_find_key(): Argument #1 ($array) must be of type array".into()),
        }
    };

    let callback = args[1];

    for (key, &val_handle) in &arr.map {
        let key_handle = vm.arena.alloc(match key {
            ArrayKey::Int(i) => Val::Int(*i),
            ArrayKey::Str(s) => Val::String((*s).clone().into()),
        });
        let res = vm
            .call_callable(callback, smallvec![val_handle, key_handle])
            .map_err(|e| e.to_string())?;
        if vm.arena.get(res).value.to_bool() {
            return Ok(key_handle);
        }
    }

    Ok(vm.arena.alloc(Val::Null))
}

pub fn php_array_is_list(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() != 1 {
        return Err("array_is_list() expects exactly 1 parameter".into());
    }

    let val = vm.arena.get(args[0]);
    let arr = match &val.value {
        Val::Array(arr) => arr,
        _ => return Err("array_is_list(): Argument #1 ($array) must be of type array".into()),
    };

    for (i, key) in arr.map.keys().enumerate() {
        match key {
            ArrayKey::Int(idx) if *idx == i as i64 => continue,
            _ => return Ok(vm.arena.alloc(Val::Bool(false))),
        }
    }

    Ok(vm.arena.alloc(Val::Bool(true)))
}

pub fn php_array_change_key_case(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() < 1 || args.len() > 2 {
        return Err("array_change_key_case() expects 1 or 2 parameters".into());
    }

    let val = vm.arena.get(args[0]);
    let arr = match &val.value {
        Val::Array(arr) => arr,
        _ => {
            return Err(
                "array_change_key_case(): Argument #1 ($array) must be of type array".into(),
            );
        }
    };

    let case = if args.len() == 2 {
        match vm.arena.get(args[1]).value {
            Val::Int(i) => i,
            _ => 0, // Default to CASE_LOWER
        }
    } else {
        0 // CASE_LOWER
    };

    let mut map = IndexMap::new();
    let mut next_free = 0;

    for (key, &val_handle) in &arr.map {
        let new_key = match key {
            ArrayKey::Int(i) => ArrayKey::Int(*i),
            ArrayKey::Str(s) => {
                let s_str = String::from_utf8_lossy(s);
                let new_s = if case == 1 {
                    // CASE_UPPER
                    s_str.to_uppercase()
                } else {
                    // CASE_LOWER
                    s_str.to_lowercase()
                };
                ArrayKey::Str(new_s.into_bytes().into())
            }
        };

        if let ArrayKey::Int(i) = new_key {
            if i >= next_free {
                next_free = i + 1;
            }
        }
        map.insert(new_key, val_handle);
    }

    Ok(vm.arena.alloc(Val::Array(
        crate::core::value::ArrayData {
            map,
            next_free,
            internal_ptr: 0,
        }
        .into(),
    )))
}

pub fn php_array_unique(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() < 1 || args.len() > 2 {
        return Err("array_unique() expects 1 or 2 parameters".into());
    }

    let val = vm.arena.get(args[0]);
    let arr = match &val.value {
        Val::Array(arr) => arr,
        _ => return Err("array_unique(): Argument #1 ($array) must be of type array".into()),
    };

    // flags are ignored for now, we use string comparison by default
    let mut seen = HashSet::new();
    let mut map = IndexMap::new();
    let mut next_free = 0;

    for (key, &val_handle) in &arr.map {
        let v = vm.arena.get(val_handle).value.clone();
        // PHP's array_unique uses string representation for comparison by default
        let s = match v {
            Val::String(s) => s.to_vec(),
            _ => v.to_php_string_bytes(),
        };

        if seen.insert(s) {
            if let ArrayKey::Int(i) = key {
                if *i >= next_free {
                    next_free = *i + 1;
                }
            }
            map.insert(key.clone(), val_handle);
        }
    }

    Ok(vm.arena.alloc(Val::Array(
        crate::core::value::ArrayData {
            map,
            next_free,
            internal_ptr: 0,
        }
        .into(),
    )))
}

pub fn php_compact(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    let mut result_map = IndexMap::new();

    fn add_to_compact(
        vm: &mut VM,
        arg_handle: Handle,
        result_map: &mut IndexMap<ArrayKey, Handle>,
    ) {
        let val = vm.arena.get(arg_handle).value.clone();
        match val {
            Val::String(s) => {
                let sym = vm.context.interner.intern(&s);
                if let Some(frame) = vm.frames.last() {
                    if let Some(&var_handle) = frame.locals.get(&sym) {
                        let key = ArrayKey::Str(s);
                        result_map.insert(key, var_handle);
                    }
                }
            }
            Val::Array(arr) => {
                for (_, &h) in &arr.map {
                    add_to_compact(vm, h, result_map);
                }
            }
            _ => {}
        }
    }

    for &arg in args {
        add_to_compact(vm, arg, &mut result_map);
    }

    Ok(vm.arena.alloc(Val::Array(
        crate::core::value::ArrayData::from(result_map).into(),
    )))
}

pub fn php_extract(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("extract() expects at least 1 parameter".into());
    }

    let arr_handle = args[0];
    let arr_val = vm.arena.get(arr_handle);
    let arr = match &arr_val.value {
        Val::Array(a) => a,
        _ => return Err("extract(): Argument #1 must be an array".into()),
    };

    // Simplified extract: always overwrite for now
    let mut count = 0;
    let frame = vm.frames.last_mut().ok_or("No active frame")?;

    for (key, &val_handle) in &arr.map {
        if let ArrayKey::Str(s) = key {
            let sym = vm.context.interner.intern(s);
            frame.locals.insert(sym, val_handle);
            count += 1;
        }
    }

    Ok(vm.arena.alloc(Val::Int(count)))
}

pub fn php_array_diff(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() < 2 {
        return Err("array_diff() expects at least 2 parameters".into());
    }

    let first_val = vm.arena.get(args[0]);
    let first_arr = match &first_val.value {
        Val::Array(a) => a,
        _ => return Err("array_diff(): Argument #1 must be an array".into()),
    };

    let mut other_values = HashSet::new();
    for &arg in &args[1..] {
        let val = vm.arena.get(arg);
        if let Val::Array(arr) = &val.value {
            for (_, &vh) in &arr.map {
                let v = vm.arena.get(vh).value.clone();
                other_values.insert(v.to_php_string_bytes());
            }
        }
    }

    let mut result_map = IndexMap::new();
    let mut next_free = 0;

    for (key, &val_handle) in &first_arr.map {
        let v = vm.arena.get(val_handle).value.clone();
        if !other_values.contains(&v.to_php_string_bytes()) {
            if let ArrayKey::Int(i) = key {
                if *i >= next_free {
                    next_free = *i + 1;
                }
            }
            result_map.insert(key.clone(), val_handle);
        }
    }

    Ok(vm.arena.alloc(Val::Array(
        crate::core::value::ArrayData {
            map: result_map,
            next_free,
            internal_ptr: 0,
        }
        .into(),
    )))
}

pub fn php_array_intersect(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() < 2 {
        return Err("array_intersect() expects at least 2 parameters".into());
    }

    let first_val = vm.arena.get(args[0]);
    let first_arr = match &first_val.value {
        Val::Array(a) => a,
        _ => return Err("array_intersect(): Argument #1 must be an array".into()),
    };

    let mut intersect_sets = Vec::new();
    for &arg in &args[1..] {
        let val = vm.arena.get(arg);
        if let Val::Array(arr) = &val.value {
            let mut set = HashSet::new();
            for (_, &vh) in &arr.map {
                let v = vm.arena.get(vh).value.clone();
                set.insert(v.to_php_string_bytes());
            }
            intersect_sets.push(set);
        }
    }

    let mut result_map = IndexMap::new();
    let mut next_free = 0;

    for (key, &val_handle) in &first_arr.map {
        let v = vm.arena.get(val_handle).value.clone();
        let v_bytes = v.to_php_string_bytes();
        let mut in_all = true;
        for set in &intersect_sets {
            if !set.contains(&v_bytes) {
                in_all = false;
                break;
            }
        }

        if in_all {
            if let ArrayKey::Int(i) = key {
                if *i >= next_free {
                    next_free = *i + 1;
                }
            }
            result_map.insert(key.clone(), val_handle);
        }
    }

    Ok(vm.arena.alloc(Val::Array(
        crate::core::value::ArrayData {
            map: result_map,
            next_free,
            internal_ptr: 0,
        }
        .into(),
    )))
}

pub fn php_array_intersect_key(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() < 2 {
        return Err("array_intersect_key() expects at least 2 parameters".into());
    }

    let first_val = vm.arena.get(args[0]);
    let first_arr = match &first_val.value {
        Val::Array(a) => a,
        _ => return Err("array_intersect_key(): Argument #1 must be an array".into()),
    };

    let mut key_sets = Vec::new();
    for &arg in &args[1..] {
        let val = vm.arena.get(arg);
        if let Val::Array(arr) = &val.value {
            let mut set = HashSet::new();
            for (key, _) in &arr.map {
                set.insert(key.clone());
            }
            key_sets.push(set);
        }
    }

    let mut result_map = IndexMap::new();
    let mut next_free = 0;

    for (key, &val_handle) in &first_arr.map {
        let mut in_all = true;
        for set in &key_sets {
            if !set.contains(key) {
                in_all = false;
                break;
            }
        }

        if in_all {
            if let ArrayKey::Int(i) = key {
                if *i >= next_free {
                    next_free = *i + 1;
                }
            }
            result_map.insert(key.clone(), val_handle);
        }
    }

    Ok(vm.arena.alloc(Val::Array(
        crate::core::value::ArrayData {
            map: result_map,
            next_free,
            internal_ptr: 0,
        }
        .into(),
    )))
}

pub fn php_sort(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("sort() expects at least 1 parameter".into());
    }

    let arr_handle = args[0];
    let arr_rc = {
        let arr_val = vm.arena.get(arr_handle);
        if let Val::Array(arr_rc) = &arr_val.value {
            arr_rc.clone()
        } else {
            return Err("sort() expects parameter 1 to be array".into());
        }
    };

    let mut entries: Vec<_> = arr_rc.map.iter().map(|(_, &v)| v).collect();
    entries.sort_by(|&a, &b| {
        let va = vm.arena.get(a).value.clone();
        let vb = vm.arena.get(b).value.clone();
        // Simplified comparison
        va.to_php_string_bytes().cmp(&vb.to_php_string_bytes())
    });

    let mut new_map = IndexMap::new();
    for (i, v) in entries.into_iter().enumerate() {
        new_map.insert(ArrayKey::Int(i as i64), v);
    }

    let mut arr_data = (*arr_rc).clone();
    arr_data.map = new_map;
    arr_data.next_free = arr_data.map.len() as i64;
    arr_data.internal_ptr = 0;

    let slot = vm.arena.get_mut(arr_handle);
    slot.value = Val::Array(std::rc::Rc::new(arr_data));
    Ok(vm.arena.alloc(Val::Bool(true)))
}

pub fn php_rsort(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("rsort() expects at least 1 parameter".into());
    }

    let arr_handle = args[0];
    let arr_rc = {
        let arr_val = vm.arena.get(arr_handle);
        if let Val::Array(arr_rc) = &arr_val.value {
            arr_rc.clone()
        } else {
            return Err("rsort() expects parameter 1 to be array".into());
        }
    };

    let mut entries: Vec<_> = arr_rc.map.iter().map(|(_, &v)| v).collect();
    entries.sort_by(|&a, &b| {
        let va = vm.arena.get(a).value.clone();
        let vb = vm.arena.get(b).value.clone();
        vb.to_php_string_bytes().cmp(&va.to_php_string_bytes())
    });

    let mut new_map = IndexMap::new();
    for (i, v) in entries.into_iter().enumerate() {
        new_map.insert(ArrayKey::Int(i as i64), v);
    }

    let mut arr_data = (*arr_rc).clone();
    arr_data.map = new_map;
    arr_data.next_free = arr_data.map.len() as i64;
    arr_data.internal_ptr = 0;

    let slot = vm.arena.get_mut(arr_handle);
    slot.value = Val::Array(std::rc::Rc::new(arr_data));
    Ok(vm.arena.alloc(Val::Bool(true)))
}

pub fn php_asort(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("asort() expects at least 1 parameter".into());
    }

    let arr_handle = args[0];
    let arr_rc = {
        let arr_val = vm.arena.get(arr_handle);
        if let Val::Array(arr_rc) = &arr_val.value {
            arr_rc.clone()
        } else {
            return Err("asort() expects parameter 1 to be array".into());
        }
    };

    let mut entries: Vec<_> = arr_rc.map.iter().map(|(k, &v)| (k.clone(), v)).collect();
    entries.sort_by(|(_, a), (_, b)| {
        let va = vm.arena.get(*a).value.clone();
        let vb = vm.arena.get(*b).value.clone();
        va.to_php_string_bytes().cmp(&vb.to_php_string_bytes())
    });

    let mut arr_data = (*arr_rc).clone();
    arr_data.map = entries.into_iter().collect();
    arr_data.internal_ptr = 0;

    let slot = vm.arena.get_mut(arr_handle);
    slot.value = Val::Array(std::rc::Rc::new(arr_data));
    Ok(vm.arena.alloc(Val::Bool(true)))
}

pub fn php_arsort(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("arsort() expects at least 1 parameter".into());
    }

    let arr_handle = args[0];
    let arr_rc = {
        let arr_val = vm.arena.get(arr_handle);
        if let Val::Array(arr_rc) = &arr_val.value {
            arr_rc.clone()
        } else {
            return Err("arsort() expects parameter 1 to be array".into());
        }
    };

    let mut entries: Vec<_> = arr_rc.map.iter().map(|(k, &v)| (k.clone(), v)).collect();
    entries.sort_by(|(_, a), (_, b)| {
        let va = vm.arena.get(*a).value.clone();
        let vb = vm.arena.get(*b).value.clone();
        vb.to_php_string_bytes().cmp(&va.to_php_string_bytes())
    });

    let mut arr_data = (*arr_rc).clone();
    arr_data.map = entries.into_iter().collect();
    arr_data.internal_ptr = 0;

    let slot = vm.arena.get_mut(arr_handle);
    slot.value = Val::Array(std::rc::Rc::new(arr_data));
    Ok(vm.arena.alloc(Val::Bool(true)))
}

pub fn php_krsort(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("krsort() expects at least 1 parameter".into());
    }

    let arr_handle = args[0];
    let arr_rc = {
        let arr_val = vm.arena.get(arr_handle);
        if let Val::Array(arr_rc) = &arr_val.value {
            arr_rc.clone()
        } else {
            return Err("krsort() expects parameter 1 to be array".into());
        }
    };

    let mut entries: Vec<_> = arr_rc.map.iter().map(|(k, &v)| (k.clone(), v)).collect();
    entries.sort_by(|(a, _), (b, _)| match (a, b) {
        (ArrayKey::Int(i1), ArrayKey::Int(i2)) => i2.cmp(i1),
        (ArrayKey::Str(s1), ArrayKey::Str(s2)) => s2.cmp(s1),
        (ArrayKey::Int(_), ArrayKey::Str(_)) => std::cmp::Ordering::Greater,
        (ArrayKey::Str(_), ArrayKey::Int(_)) => std::cmp::Ordering::Less,
    });

    let mut arr_data = (*arr_rc).clone();
    arr_data.map = entries.into_iter().collect();
    arr_data.internal_ptr = 0;

    let slot = vm.arena.get_mut(arr_handle);
    slot.value = Val::Array(std::rc::Rc::new(arr_data));
    Ok(vm.arena.alloc(Val::Bool(true)))
}

pub fn php_usort(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() != 2 {
        return Err("usort() expects exactly 2 parameters".into());
    }

    let arr_handle = args[0];
    let callback = args[1];

    let arr_rc = {
        let arr_val = vm.arena.get(arr_handle);
        if let Val::Array(arr_rc) = &arr_val.value {
            arr_rc.clone()
        } else {
            return Err("usort() expects parameter 1 to be array".into());
        }
    };

    let mut entries: Vec<_> = arr_rc.map.iter().map(|(_, &v)| v).collect();

    // We need to handle errors in sort_by, but sort_by doesn't allow it.
    // We'll use a stable sort and collect errors if any.
    let mut error = None;
    entries.sort_by(|&a, &b| {
        if error.is_some() {
            return std::cmp::Ordering::Equal;
        }
        match vm.call_callable(callback, smallvec![a, b]) {
            Ok(res_handle) => {
                let res_val = vm.arena.get(res_handle).value.clone();
                let i = res_val.to_int();
                if i < 0 {
                    std::cmp::Ordering::Less
                } else if i > 0 {
                    std::cmp::Ordering::Greater
                } else {
                    std::cmp::Ordering::Equal
                }
            }
            Err(e) => {
                error = Some(e.to_string());
                std::cmp::Ordering::Equal
            }
        }
    });

    if let Some(e) = error {
        return Err(e);
    }

    let mut new_map = IndexMap::new();
    for (i, v) in entries.into_iter().enumerate() {
        new_map.insert(ArrayKey::Int(i as i64), v);
    }

    let mut arr_data = (*arr_rc).clone();
    arr_data.map = new_map;
    arr_data.next_free = arr_data.map.len() as i64;
    arr_data.internal_ptr = 0;

    let slot = vm.arena.get_mut(arr_handle);
    slot.value = Val::Array(std::rc::Rc::new(arr_data));
    Ok(vm.arena.alloc(Val::Bool(true)))
}

pub fn php_array_splice(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() < 2 || args.len() > 4 {
        return Err("array_splice() expects between 2 and 4 parameters".into());
    }

    let arr_handle = args[0];
    let offset = match vm.arena.get(args[1]).value {
        Val::Int(i) => i,
        _ => return Err("array_splice(): Argument #2 ($offset) must be of type int".into()),
    };

    let length = if args.len() >= 3 {
        match vm.arena.get(args[2]).value {
            Val::Int(i) => Some(i),
            Val::Null => None,
            _ => {
                return Err(
                    "array_splice(): Argument #3 ($length) must be of type int or null".into(),
                );
            }
        }
    } else {
        None
    };

    let replacement = if args.len() == 4 {
        let r_val = vm.arena.get(args[3]);
        match &r_val.value {
            Val::Array(a) => Some(a.clone()),
            _ => {
                // If not an array, wrap it in one
                let mut m = IndexMap::new();
                m.insert(ArrayKey::Int(0), args[3]);
                Some(crate::core::value::ArrayData::from(m).into())
            }
        }
    } else {
        None
    };

    let arr_val = vm.arena.get(arr_handle);
    if let Val::Array(arr_rc) = &arr_val.value {
        let total_len = arr_rc.map.len() as i64;
        let start = if offset >= 0 {
            offset
        } else {
            total_len + offset
        };
        let start = start.max(0).min(total_len) as usize;

        let end = if let Some(l) = length {
            if l >= 0 {
                (start as i64 + l).min(total_len)
            } else {
                (total_len + l).max(start as i64)
            }
        } else {
            total_len
        } as usize;

        let mut removed_map = IndexMap::new();
        let mut removed_next_free = 0;

        let mut new_map = IndexMap::new();
        let mut next_int = 0;

        // Elements before splice
        for i in 0..start {
            let (key, &vh) = arr_rc.map.get_index(i).unwrap();
            let new_key = match key {
                ArrayKey::Int(_) => {
                    let k = ArrayKey::Int(next_int);
                    next_int += 1;
                    k
                }
                ArrayKey::Str(s) => ArrayKey::Str(s.clone()),
            };
            new_map.insert(new_key, vh);
        }

        // Removed elements
        for i in start..end {
            let (_, &vh) = arr_rc.map.get_index(i).unwrap();
            removed_map.insert(ArrayKey::Int(removed_next_free), vh);
            removed_next_free += 1;
        }

        // Replacement elements
        if let Some(repl) = replacement {
            for (_, &vh) in &repl.map {
                new_map.insert(ArrayKey::Int(next_int), vh);
                next_int += 1;
            }
        }

        // Elements after splice
        for i in end..(total_len as usize) {
            let (key, &vh) = arr_rc.map.get_index(i).unwrap();
            let new_key = match key {
                ArrayKey::Int(_) => {
                    let k = ArrayKey::Int(next_int);
                    next_int += 1;
                    k
                }
                ArrayKey::Str(s) => ArrayKey::Str(s.clone()),
            };
            new_map.insert(new_key, vh);
        }

        let mut arr_data = (**arr_rc).clone();
        arr_data.map = new_map;
        arr_data.next_free = next_int;
        arr_data.internal_ptr = 0;

        let slot = vm.arena.get_mut(arr_handle);
        slot.value = Val::Array(std::rc::Rc::new(arr_data));

        Ok(vm.arena.alloc(Val::Array(
            crate::core::value::ArrayData {
                map: removed_map,
                next_free: removed_next_free,
                internal_ptr: 0,
            }
            .into(),
        )))
    } else {
        Err("array_splice() expects parameter 1 to be array".into())
    }
}
