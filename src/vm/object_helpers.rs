//! Object Creation Helpers
//!
//! Provides utilities for creating PHP objects with properties in a concise way.
//! These helpers are inspired by PHP's internal object creation patterns found in
//! `$PHP_SRC_PATH/Zend/zend_objects_API.c` and reflection implementations.
//!
//! ## Constructor Behavior
//!
//! **IMPORTANT:** These helpers create objects WITHOUT calling `__construct()`.
//!
//! This matches PHP's internal object creation pattern used by:
//! - `zend_objects_new()` - Core object allocation without constructor
//! - Reflection classes (ReflectionMethod, ReflectionClass, etc.)
//! - Built-in extensions creating internal objects
//! - `ReflectionClass::newInstanceWithoutConstructor()`
//!
//! If you need to create objects that should call `__construct()` (equivalent to
//! PHP's `object_init_with_constructor()` or normal `new ClassName()`), you should
//! use the VM's normal instantiation mechanisms instead.
//!
//! ## When to Use These Helpers
//!
//! Use these helpers when you need to:
//! - Create internal objects with pre-set properties (like Reflection* classes)
//! - Bypass constructor logic for internal implementation details
//! - Create objects where properties are set directly by the runtime
//! - Implement internal APIs that need object creation without side effects
//!
//! ## Usage
//!
//! ```ignore
//! use crate::vm::object_helpers::create_object;
//!
//! // Create an object with properties (NO constructor call)
//! let obj = create_object!(vm, b"ReflectionMethod", {
//!     b"class" => Val::String(Rc::new(b"MyClass".to_vec())),
//!     b"name" => Val::String(Rc::new(b"myMethod".to_vec())),
//! })?;
//!
//! // Or use the function directly
//! let obj = create_object_with_properties(
//!     vm,
//!     b"ReflectionMethod",
//!     &[
//!         (b"class", Val::String(Rc::new(b"value".to_vec()))),
//!         (b"name", Val::String(Rc::new(b"value".to_vec()))),
//!     ],
//! )?;
//! ```

use crate::core::value::{Handle, ObjectData, Val};
use crate::vm::engine::VM;
use std::collections::HashSet;

/// Create a PHP object with the specified class and properties
///
/// **IMPORTANT:** This function does NOT call `__construct()`. It creates objects
/// in the same way as PHP's internal `zend_objects_new()` function.
///
/// This is the correct approach for:
/// - Creating internal objects (Reflection*, DateTime*, etc.)
/// - Implementing internal APIs that bypass user constructors
/// - Setting up objects with predefined properties before any user code runs
///
/// For normal object instantiation that requires `__construct()` to be called
/// (equivalent to `new ClassName()` or PHP's `object_init_with_constructor()`),
/// use the VM's standard object instantiation flow instead.
///
/// # Arguments
///
/// * `vm` - Mutable reference to the VM
/// * `class_name` - Class name as a byte slice (e.g., `b"MyClass"`)
/// * `properties` - Slice of (property_name, property_value) tuples
///
/// # Returns
///
/// A `Result` containing the `Handle` to the created object, or a `String` error message
///
/// # Example
///
/// ```ignore
/// // Create a ReflectionMethod without calling __construct
/// let obj = create_object_with_properties(
///     vm,
///     b"ReflectionMethod",
///     &[
///         (b"class", Val::String(Rc::new(class_name_bytes))),
///         (b"name", Val::String(Rc::new(method_name_bytes))),
///     ],
/// )?;
/// ```
///
/// # Implementation Details
///
/// The function performs the following steps:
/// 1. Interns the class name into a `Symbol`
/// 2. Creates an empty `ObjectData` structure
/// 3. Allocates the object payload and object handle to the arena
/// 4. Pre-allocates all property value handles
/// 5. Inserts properties into the object's property map
///
/// Pre-allocation of property handles (step 4) is necessary to avoid multiple
/// mutable borrows of `vm.arena`, which would violate Rust's borrowing rules.
///
/// # PHP Source Reference
///
/// This implementation mirrors PHP's `zend_objects_new()` pattern, NOT
/// `object_init_with_constructor()`. See `Zend/zend_objects.c` and
/// `Zend/zend_API.c` in the PHP source for the distinction.
#[inline]
pub fn create_object_with_properties(
    vm: &mut VM,
    class_name: &[u8],
    properties: &[(&[u8], Val)],
) -> Result<Handle, String> {
    // Intern the class name
    let class_sym = vm.context.interner.intern(class_name);

    // Create object payload
    let obj_data = ObjectData {
        class: class_sym,
        properties: indexmap::IndexMap::new(),
        internal: None,
        dynamic_properties: HashSet::new(),
    };
    let obj_payload_handle = vm.arena.alloc(Val::ObjPayload(obj_data));
    let obj_handle = vm.arena.alloc(Val::Object(obj_payload_handle));

    // Pre-allocate all property handles to avoid multiple mutable borrows
    let mut prop_handles = Vec::with_capacity(properties.len());
    for (prop_name, prop_val) in properties {
        let prop_sym = vm.context.interner.intern(prop_name);
        let prop_handle = vm.arena.alloc(prop_val.clone());
    **IMPORTANT:** This function does NOT call `__construct()`.
///
/// This is useful when you need to create an object and set properties
/// later using standard property access operations, without triggering
/// constructor logic. Matches PHP's `zend_objects_new()` behavior.
///
/// # Arguments
///
/// * `vm` - Mutable reference to the VM
/// * `class_name` - Class name as a byte slice (e.g., `b"MyClass"`)
///
/// # Returns
///
/// A `Result` containing the `Handle` to the created object, or a `String` error message
///
/// # Example
///
/// ```ignore
/// // Create object without calling __construct
/// This is useful when you need to create an object and set properties
/// later using standard property access operations.
///
/// # Arguments
///
/// * `vm` - Mutable reference to the VM
/// * `class_name` - Class name as a byte slice (e.g., `b"MyClass"`)
///
/// # Returns
///
/// A `Result` containing the `Handle` to the created object, or a `String` error message
///
/// # Example
///
/// ```ignore
/// let obj = create_empty_object(vm, b"MyClass")?;
/// // Set properties later through normal property access
/// ```
#[inline]
pub fn create_empty_object(vm: &mut VM, class_name: &[u8]) -> Result<Handle, String> {
    let class_sym = vm.context.interner.intern(class_name);

    let obj_data = ObjectData {
        class: class_sym,
        properties: indexmap::IndexMap::new(),
        internal: None,
        dynamic_properties: HashSet::new(),
    };
    let obj_payload_handle = vm.arena.alloc(Val::ObjPayload(obj_data));
    let obj_handle = vm.arena.alloc(Val::Object(obj_payload_handle));

    Ok(obj_handle)
}

/// Macro for creating objects with a more ergonomic syntax
///
/// This macro provides a convenient way to create objects with properties
/// using a map-like syntax similar to PHP array literals.
///
/// # Syntax
///
/// ```ignore
/// create_object!(vm, b"ClassName", {
///     b"property1" => value1,
///     b"property2" => value2,
///     // ...
/// })
/// ```
///
/// # Example
///
/// ```ignore
/// use crate::vm::object_helpers::create_object;
/// use std::rc::Rc;
///
/// let obj = create_object!(vm, b"ReflectionMethod", {
///     b"class" => Val::String(Rc::new(b"MyClass".to_vec())),
///     b"method" => Val::String(Rc::new(b"myMethod".to_vec())),
///     b"modifiers" => Val::Int(1),
/// })?;
/// ```
#[macro_export]
macro_rules! create_object {
    ($vm:expr, $class:expr, { $($prop:expr => $val:expr),* $(,)? }) => {
        $crate::vm::object_helpers::create_object_with_properties(
            $vm,
            $class,
            &[
                $(($prop, $val),)*
            ],
        )
    };
}

// Note: Unit tests for this module should be integration tests
// that properly initialize the VM, as the VM structure is complex
// and not suitable for direct construction in unit tests.
