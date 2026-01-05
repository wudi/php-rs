# Object Creation Helpers Usage Guide

This guide demonstrates how to use the general-purpose object creation helpers provided in `src/vm/object_helpers.rs`.

## Overview

The object creation helpers provide a clean, concise way to create PHP objects with properties, eliminating boilerplate code and making the codebase more maintainable.

## Benefits

- **Less boilerplate**: ~30 lines of code reduced to ~7 lines
- **Type-safe**: Full Rust type checking maintained
- **Borrowing-safe**: Handles Rust's borrowing rules correctly
- **Consistent**: Same pattern used throughout the codebase
- **Well-documented**: Clear examples and PHP source references

## API Reference

### Function: `create_object_with_properties`

Creates an object with the specified class name and properties.

```rust
pub fn create_object_with_properties(
    vm: &mut VM,
    class_name: &[u8],
    properties: &[(&[u8], Val)],
) -> Result<Handle, String>
```

### Function: `create_empty_object`

Creates an empty object (no properties).

```rust
pub fn create_empty_object(
    vm: &mut VM,
    class_name: &[u8],
) -> Result<Handle, String>
```

### Macro: `create_object!`

Provides a more ergonomic syntax for creating objects.

```rust
create_object!(vm, b"ClassName", {
    b"property1" => value1,
    b"property2" => value2,
})
```

## Usage Examples

### Example 1: Creating Reflection Objects

The most common use case - creating reflection objects:

```rust
use crate::vm::object_helpers::create_object_with_properties;
use std::rc::Rc;

// Create a ReflectionMethod object
let method_obj = create_object_with_properties(
    vm,
    b"ReflectionMethod",
    &[
        (b"class", Val::String(Rc::new(b"MyClass".to_vec()))),
        (b"method", Val::String(Rc::new(b"myMethod".to_vec()))),
    ],
)?;

// Create a ReflectionProperty object
let prop_obj = create_object_with_properties(
    vm,
    b"ReflectionProperty",
    &[
        (b"class", Val::String(Rc::new(b"MyClass".to_vec()))),
        (b"name", Val::String(Rc::new(b"myProperty".to_vec()))),
    ],
)?;

// Create a ReflectionParameter object
let param_obj = create_object_with_properties(
    vm,
    b"ReflectionParameter",
    &[
        (b"function", Val::String(Rc::new(b"myFunction".to_vec()))),
        (b"name", Val::String(Rc::new(b"$param".to_vec()))),
        (b"position", Val::Int(0)),
    ],
)?;
```

### Example 2: Using the Macro

More concise syntax with the macro (must import):

```rust
use crate::create_object;
use std::rc::Rc;

let obj = create_object!(vm, b"DateTime", {
    b"date" => Val::String(Rc::new(b"2024-01-01".to_vec())),
    b"timezone" => Val::String(Rc::new(b"UTC".to_vec())),
})?;
```

### Example 3: Creating Exception Objects

```rust
use crate::vm::object_helpers::create_object_with_properties;
use std::rc::Rc;

let exception = create_object_with_properties(
    vm,
    b"Exception",
    &[
        (b"message", Val::String(Rc::new(b"Something went wrong".to_vec()))),
        (b"code", Val::Int(500)),
        (b"file", Val::String(Rc::new(file_path.as_bytes().to_vec()))),
        (b"line", Val::Int(line_number)),
    ],
)?;
```

### Example 4: Creating stdClass Objects

```rust
use crate::vm::object_helpers::create_object_with_properties;
use std::rc::Rc;

// Create a stdClass object with dynamic properties
let obj = create_object_with_properties(
    vm,
    b"stdClass",
    &[
        (b"id", Val::Int(123)),
        (b"name", Val::String(Rc::new(b"John Doe".to_vec()))),
        (b"active", Val::Bool(true)),
    ],
)?;
```

### Example 5: Empty Object (Set Properties Later)

```rust
use crate::vm::object_helpers::create_empty_object;

// Create empty object
let obj = create_empty_object(vm, b"MyClass")?;

// Set properties later through normal property access
// (useful when properties are set conditionally or in a loop)
```

## Migration Guide

### Before (Old Code)

```rust
// Old boilerplate (30+ lines)
let reflection_method_class = vm.context.interner.intern(b"ReflectionMethod");

let obj_data = ObjectData {
    class: reflection_method_class,
    properties: indexmap::IndexMap::new(),
    internal: None,
    dynamic_properties: std::collections::HashSet::new(),
};
let obj_payload_handle = vm.arena.alloc(Val::ObjPayload(obj_data));
let obj_handle = vm.arena.alloc(Val::Object(obj_payload_handle));

let class_sym = vm.context.interner.intern(b"class");
let method_sym = vm.context.interner.intern(b"method");

let class_name_bytes = lookup_symbol(vm, class_name).to_vec();
let method_name_bytes = b"__construct".to_vec();

let class_name_handle = vm.arena.alloc(Val::String(Rc::new(class_name_bytes)));
let method_name_handle = vm.arena.alloc(Val::String(Rc::new(method_name_bytes)));

if let Val::ObjPayload(obj_data) = &mut vm.arena.get_mut(obj_payload_handle).value {
    obj_data.properties.insert(class_sym, class_name_handle);
    obj_data.properties.insert(method_sym, method_name_handle);
}

Ok(obj_handle)
```

### After (New Code)

```rust
// New concise code (7 lines)
use crate::vm::object_helpers::create_object_with_properties;

let class_name_bytes = lookup_symbol(vm, class_name).to_vec();
let method_name_bytes = b"__construct".to_vec();

create_object_with_properties(
    vm,
    b"ReflectionMethod",
    &[
        (b"class", Val::String(Rc::new(class_name_bytes))),
        (b"method", Val::String(Rc::new(method_name_bytes))),
    ],
)
```

## Use Cases Throughout the Codebase

### Reflection Extension (`src/builtins/reflection.rs`)

✅ Already migrated:
- `reflection_class_get_constructor()` - Creates ReflectionMethod
- `reflection_class_get_method()` - Creates ReflectionMethod

🔜 Can be migrated:
- `reflection_class_get_property()` - Create ReflectionProperty
- `reflection_class_get_reflection_constant()` - Create ReflectionClassConstant
- `reflection_enum_get_case()` - Create ReflectionEnumUnitCase
- Many other reflection object creation points

### DateTime Extension (`src/builtins/datetime.rs`)

```rust
// Creating DateTime objects
let dt_obj = create_object_with_properties(
    vm,
    b"DateTime",
    &[
        (b"date", Val::String(Rc::new(formatted_date))),
        (b"timezone", Val::String(Rc::new(timezone_name))),
    ],
)?;
```

### Exception Handling (Various modules)

```rust
// Creating exception objects dynamically
let exception = create_object_with_properties(
    vm,
    exception_class_name,
    &[
        (b"message", Val::String(Rc::new(error_message))),
        (b"code", Val::Int(error_code)),
    ],
)?;
```

### stdClass Creation (Array to object casts, etc.)

```rust
// Convert array to stdClass
let obj = create_object_with_properties(
    vm,
    b"stdClass",
    &array_properties_converted,
)?;
```

## Best Practices

1. **Import at module level**:
   ```rust
   use crate::vm::object_helpers::create_object_with_properties;
   ```

2. **Prepare data before creating object**:
   ```rust
   // Good: Prepare all data first
   let class_bytes = lookup_symbol(vm, class_name).to_vec();
   let method_bytes = method_name.as_bytes().to_vec();
   
   let obj = create_object_with_properties(vm, b"MyClass", &[
       (b"class", Val::String(Rc::new(class_bytes))),
       (b"method", Val::String(Rc::new(method_bytes))),
   ])?;
   ```

3. **Use meaningful variable names**:
   ```rust
   // Good
   let reflection_method = create_object_with_properties(...)?;
   
   // Less clear
   let obj = create_object_with_properties(...)?;
   ```

4. **Consider the macro for complex objects**:
   ```rust
   // When many properties, the macro is clearer
   use crate::create_object;
   
   let obj = create_object!(vm, b"ComplexClass", {
       b"prop1" => val1,
       b"prop2" => val2,
       b"prop3" => val3,
       b"prop4" => val4,
   })?;
   ```

## Performance Notes

- **Zero-cost abstraction**: The helper compiles down to the same code as manual creation
- **No heap allocations beyond necessary**: Uses the VM's arena allocator
- **Borrow-checker friendly**: Pre-allocates handles to avoid multiple mutable borrows

## Related PHP Source References

- `$PHP_SRC_PATH/Zend/zend_objects_API.c` - Object creation API
- `$PHP_SRC_PATH/Zend/zend_reflection.c` - Reflection object creation
- `$PHP_SRC_PATH/ext/date/php_date.c` - DateTime object creation

## Summary

The object creation helpers provide a standardized, maintainable way to create PHP objects throughout the codebase. They eliminate boilerplate while maintaining type safety and performance.

**Key takeaway**: Use `create_object_with_properties()` whenever you need to create an object with initial properties. It's faster to write, easier to read, and less error-prone.
