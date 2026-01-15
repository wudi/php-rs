use crate::core::value::{ArrayData, ArrayKey, Handle, Val};
use crate::vm::engine::VM;
use pcre2::bytes::{Regex, Captures};
use smallvec::smallvec;
use std::rc::Rc;

pub fn preg_match(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    // args: pattern, subject, matches (ref), flags, offset
    if args.len() < 2 {
        return Err("preg_match expects at least 2 arguments".into());
    }

    let pattern_handle = args[0];
    let subject_handle = args[1];

    let pattern_str = match &vm.arena.get(pattern_handle).value {
        Val::String(s) => s.clone(),
        _ => return Err("preg_match pattern must be a string".into()),
    };

    let subject_str = match &vm.arena.get(subject_handle).value {
        Val::String(s) => s.clone(),
        _ => Rc::new(
            vm.convert_to_string(subject_handle)
                .map_err(|e| e.to_string())?,
        ),
    };

    let (pattern_bytes, _flags) = parse_php_pattern(&pattern_str)?;

    let regex = Regex::new(&String::from_utf8_lossy(&pattern_bytes))
        .map_err(|e| format!("Invalid regex: {}", e))?;

    // If matches array is provided, populate it
    if args.len() >= 3 {
        let matches_handle = args[2];
        let captures = regex.captures(&subject_str)
            .map_err(|e| format!("Regex execution error: {}", e))?;

        if let Some(captures) = captures {
            let mut match_array = ArrayData::new();
            let mut indexed_handles = Vec::new();

            for i in 0..captures.len() {
                let handle = if let Some(m) = captures.get(i) {
                    let match_str = m.as_bytes().to_vec();
                    vm.arena.alloc(Val::String(Rc::new(match_str)))
                } else {
                    vm.arena.alloc(Val::String(Rc::new(Vec::new())))
                };
                match_array.insert(ArrayKey::Int(i as i64), handle);
                indexed_handles.push(handle);
            }

            for (idx, name) in regex.capture_names().iter().enumerate() {
                if let Some(name) = name.as_deref() {
                    if let Some(&handle) = indexed_handles.get(idx) {
                        match_array.insert(
                            ArrayKey::Str(Rc::new(name.as_bytes().to_vec())),
                            handle,
                        );
                    }
                }
            }

            // Update the referenced matches variable
            if vm.arena.get(matches_handle).is_ref {
                let slot = vm.arena.get_mut(matches_handle);
                slot.value = Val::Array(Rc::new(match_array));
            }

            // Match found
            Ok(vm.arena.alloc(Val::Int(1)))
        } else {
            // No match
            Ok(vm.arena.alloc(Val::Int(0)))
        }
    } else {
        let is_match = regex.is_match(&subject_str)
             .map_err(|e| format!("Regex execution error: {}", e))?;
        Ok(vm.arena.alloc(Val::Int(if is_match { 1 } else { 0 })))
    }
}

pub fn preg_replace(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    // args: pattern, replacement, subject, limit, count
    if args.len() < 3 {
        return Err("preg_replace expects at least 3 arguments".into());
    }

    let pattern_handle = args[0];
    let replacement_handle = args[1];
    let subject_handle = args[2];
    
    let limit = if args.len() >= 4 {
        match vm.arena.get(args[3]).value {
            Val::Int(l) => l,
            _ => -1,
        }
    } else {
        -1
    };

    let pattern_str = match &vm.arena.get(pattern_handle).value {
        Val::String(s) => s.clone(),
        _ => return Err("preg_replace pattern must be a string".into()),
    };

    let replacement_str = match &vm.arena.get(replacement_handle).value {
        Val::String(s) => s.clone(),
        _ => return Err("preg_replace replacement must be a string".into()),
    };

    let subject_str = match &vm.arena.get(subject_handle).value {
        Val::String(s) => s.clone(),
        _ => return Err("preg_replace subject must be a string".into()),
    };

    let (pattern_bytes, _flags) = parse_php_pattern(&pattern_str)?;

    let regex = Regex::new(&String::from_utf8_lossy(&pattern_bytes))
        .map_err(|e| format!("Invalid regex: {}", e))?;

    let mut result = Vec::new();
    let mut last_end = 0;
    let mut count = 0;

    for captures in regex.captures_iter(&subject_str) {
        let captures = captures.map_err(|e| format!("Regex error: {}", e))?;
        
        // captures.get(0) is the whole match
        if let Some(m) = captures.get(0) {
            if limit != -1 && count >= limit {
                break;
            }

            result.extend_from_slice(&subject_str[last_end..m.start()]);
            
            let replaced = interpolate_replacement(&replacement_str, &captures);
            result.extend_from_slice(&replaced);
            
            last_end = m.end();
            count += 1;
        }
    }
    
    result.extend_from_slice(&subject_str[last_end..]);

    // Update count variable if provided
    if args.len() >= 5 {
        let count_handle = args[4];
        if vm.arena.get(count_handle).is_ref {
            let slot = vm.arena.get_mut(count_handle);
            slot.value = Val::Int(count);
        }
    }

    Ok(vm.arena.alloc(Val::String(Rc::new(result))))
}

pub fn preg_match_all(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    // args: pattern, subject, matches (ref), flags, offset
    if args.len() < 2 {
        return Err("preg_match_all expects at least 2 arguments".into());
    }

    let pattern_handle = args[0];
    let subject_handle = args[1];

    let flags = if args.len() >= 4 {
        match vm.arena.get(args[3]).value {
            Val::Int(value) => value,
            _ => 0,
        }
    } else {
        0
    };

    let offset = if args.len() >= 5 {
        match vm.arena.get(args[4]).value {
            Val::Int(value) if value > 0 => value as usize,
            _ => 0,
        }
    } else {
        0
    };

    let pattern_str = match &vm.arena.get(pattern_handle).value {
        Val::String(s) => s.clone(),
        _ => return Err("preg_match_all pattern must be a string".into()),
    };

    let subject_str = match &vm.arena.get(subject_handle).value {
        Val::String(s) => s.clone(),
        _ => Rc::new(
            vm.convert_to_string(subject_handle)
                .map_err(|e| e.to_string())?,
        ),
    };

    let (pattern_bytes, _flags) = parse_php_pattern(&pattern_str)?;

    let regex = Regex::new(&String::from_utf8_lossy(&pattern_bytes))
        .map_err(|e| format!("Invalid regex: {}", e))?;

    let set_order = (flags & 2) != 0;
    let offset_capture = (flags & (1 << 8)) != 0;
    let unmatched_as_null = (flags & (1 << 9)) != 0;

    let mut count = 0i64;
    let mut matches_array = ArrayData::new();

    if set_order {
        let mut match_index = 0i64;
        for captures in regex.captures_iter(&subject_str) {
            let captures = captures.map_err(|e| format!("Regex execution error: {}", e))?;
            let Some(m0) = captures.get(0) else {
                continue;
            };
            if m0.start() < offset {
                continue;
            }

            let mut row = ArrayData::new();
            for i in 0..captures.len() {
                let capture = captures.get(i);
                let val = capture_to_val(vm, capture, offset_capture, unmatched_as_null);
                row.insert(ArrayKey::Int(i as i64), val);
            }

            let row_handle = vm.arena.alloc(Val::Array(Rc::new(row)));
            matches_array.insert(ArrayKey::Int(match_index), row_handle);
            match_index += 1;
            count += 1;
        }
    } else {
        let mut group_arrays: Vec<ArrayData> = Vec::new();
        let mut match_index = 0i64;

        for captures in regex.captures_iter(&subject_str) {
            let captures = captures.map_err(|e| format!("Regex execution error: {}", e))?;
            let Some(m0) = captures.get(0) else {
                continue;
            };
            if m0.start() < offset {
                continue;
            }

            if group_arrays.is_empty() {
                group_arrays = (0..captures.len()).map(|_| ArrayData::new()).collect();
            }

            for i in 0..captures.len() {
                let capture = captures.get(i);
                let val = capture_to_val(vm, capture, offset_capture, unmatched_as_null);
                group_arrays[i].insert(ArrayKey::Int(match_index), val);
            }

            match_index += 1;
            count += 1;
        }

        for (i, group) in group_arrays.into_iter().enumerate() {
            let group_handle = vm.arena.alloc(Val::Array(Rc::new(group)));
            matches_array.insert(ArrayKey::Int(i as i64), group_handle);
        }
    }

    if args.len() >= 3 {
        let matches_handle = args[2];
        if vm.arena.get(matches_handle).is_ref {
            let slot = vm.arena.get_mut(matches_handle);
            slot.value = Val::Array(Rc::new(matches_array));
        }
    }

    Ok(vm.arena.alloc(Val::Int(count)))
}

pub fn preg_replace_callback(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() < 3 {
        return Err("preg_replace_callback expects at least 3 arguments".into());
    }

    let pattern_handle = args[0];
    let callback_handle = args[1];
    let subject_handle = args[2];

    let limit = if args.len() >= 4 {
        match vm.arena.get(args[3]).value {
            Val::Int(l) => l,
            _ => -1,
        }
    } else {
        -1
    };

    let pattern_str = match &vm.arena.get(pattern_handle).value {
        Val::String(s) => s.clone(),
        _ => return Err("preg_replace_callback pattern must be a string".into()),
    };

    let subject_str = match &vm.arena.get(subject_handle).value {
        Val::String(s) => s.clone(),
        _ => Rc::new(
            vm.convert_to_string(subject_handle)
                .map_err(|e| e.to_string())?,
        ),
    };

    let (pattern_bytes, _flags) = parse_php_pattern(&pattern_str)?;

    let regex = Regex::new(&String::from_utf8_lossy(&pattern_bytes))
        .map_err(|e| format!("Invalid regex: {}", e))?;

    let mut result = Vec::new();
    let mut last_end = 0;
    let mut count = 0;

    for captures in regex.captures_iter(&subject_str) {
        let captures = captures.map_err(|e| format!("Regex error: {}", e))?;

        if let Some(m) = captures.get(0) {
            if limit != -1 && count >= limit {
                break;
            }

            result.extend_from_slice(&subject_str[last_end..m.start()]);

            let mut match_array = ArrayData::new();
            for i in 0..captures.len() {
                if let Some(mat) = captures.get(i) {
                    let match_str = mat.as_bytes().to_vec();
                    let val = vm.arena.alloc(Val::String(Rc::new(match_str)));
                    match_array.insert(ArrayKey::Int(i as i64), val);
                }
            }

            let matches_handle = vm.arena.alloc(Val::Array(Rc::new(match_array)));
            let callback_result = vm
                .call_callable(callback_handle, smallvec![matches_handle])
                .map_err(|e| e.to_string())?;
            let replacement = vm.value_to_string(callback_result)?;

            result.extend_from_slice(&replacement);
            last_end = m.end();
            count += 1;
        }
    }

    result.extend_from_slice(&subject_str[last_end..]);

    if args.len() >= 5 {
        let count_handle = args[4];
        if vm.arena.get(count_handle).is_ref {
            let slot = vm.arena.get_mut(count_handle);
            slot.value = Val::Int(count);
        }
    }

    Ok(vm.arena.alloc(Val::String(Rc::new(result))))
}

fn interpolate_replacement(replacement: &[u8], captures: &Captures) -> Vec<u8> {
    let mut result = Vec::new();
    let mut i = 0;
    while i < replacement.len() {
        if replacement[i] == b'$' || replacement[i] == b'\\' {
            // Check for digit
            if i + 1 < replacement.len() {
                let next_char = replacement[i+1];
                 if next_char.is_ascii_digit() {
                    let mut digit_end = i + 2;
                    // Support $0 to $99
                    if digit_end < replacement.len() && replacement[digit_end].is_ascii_digit() {
                        digit_end += 1;
                    }
                    
                    let group_idx_str = std::str::from_utf8(&replacement[i+1..digit_end]).unwrap_or("0");
                    let group_idx: usize = group_idx_str.parse().unwrap_or(0);
                    
                    if let Some(m) = captures.get(group_idx) {
                        result.extend_from_slice(m.as_bytes());
                    }
                    
                    i = digit_end;
                    continue;
                }
            }
        }
        result.push(replacement[i]);
        i += 1;
    }
    result
}

fn capture_to_val(
    vm: &mut VM,
    capture: Option<pcre2::bytes::Match>,
    offset_capture: bool,
    unmatched_as_null: bool,
) -> Handle {
    if let Some(m) = capture {
        let match_val = vm.arena.alloc(Val::String(Rc::new(m.as_bytes().to_vec())));
        if offset_capture {
            let mut pair = ArrayData::new();
            pair.insert(ArrayKey::Int(0), match_val);
            pair.insert(ArrayKey::Int(1), vm.arena.alloc(Val::Int(m.start() as i64)));
            vm.arena.alloc(Val::Array(Rc::new(pair)))
        } else {
            match_val
        }
    } else if unmatched_as_null {
        vm.arena.alloc(Val::Null)
    } else {
        vm.arena.alloc(Val::String(Rc::new(Vec::new())))
    }
}

pub fn preg_split(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    // args: pattern, subject, limit, flags
    if args.len() < 2 {
        return Err("preg_split expects at least 2 arguments".into());
    }

    let pattern_handle = args[0];
    let subject_handle = args[1];

    let pattern_str = match &vm.arena.get(pattern_handle).value {
        Val::String(s) => s.clone(),
        _ => return Err("preg_split pattern must be a string".into()),
    };

    let subject_str = match &vm.arena.get(subject_handle).value {
        Val::String(s) => s.clone(),
        _ => return Err("preg_split subject must be a string".into()),
    };

    let (pattern_bytes, _flags) = parse_php_pattern(&pattern_str)?;

    let regex = Regex::new(&String::from_utf8_lossy(&pattern_bytes))
        .map_err(|e| format!("Invalid regex: {}", e))?;

    let mut result = ArrayData::new();
    let mut last_end = 0;
    let mut index = 0i64;

    for m in regex.find_iter(&subject_str) {
        let m = m.map_err(|e| format!("Regex find error: {}", e))?;
        // Add the part before the match
        let before = subject_str[last_end..m.start()].to_vec();
        let val = vm.arena.alloc(Val::String(Rc::new(before)));
        result.insert(ArrayKey::Int(index), val);
        index += 1;
        last_end = m.end();
    }

    // Add the remaining part
    let remaining = subject_str[last_end..].to_vec();
    let val = vm.arena.alloc(Val::String(Rc::new(remaining)));
    result.insert(ArrayKey::Int(index), val);

    Ok(vm.arena.alloc(Val::Array(Rc::new(result))))
}

pub fn preg_quote(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("preg_quote expects at least 1 argument".into());
    }
    let str_val = match &vm.arena.get(args[0]).value {
        Val::String(s) => s.clone(),
        _ => return Err("preg_quote expects string".into()),
    };

    Ok(vm.arena.alloc(Val::String(str_val)))
}

fn parse_php_pattern(pattern: &[u8]) -> Result<(Vec<u8>, String), String> {
    if pattern.len() < 2 {
        return Err("Empty regex".into());
    }

    let delimiter = pattern[0];
    // Find closing delimiter
    let mut end = 0;
    let mut i = 1;
    while i < pattern.len() {
        if pattern[i] == b'\\' {
            i += 2;
            continue;
        }
        if pattern[i] == delimiter {
            end = i;
            break;
        }
        i += 1;
    }

    if end == 0 {
        return Err("No ending delimiter found".into());
    }

    let regex_part = pattern[1..end].to_vec();
    let flags_part = String::from_utf8_lossy(&pattern[end + 1..]).to_string();

    Ok((regex_part, flags_part))
}
