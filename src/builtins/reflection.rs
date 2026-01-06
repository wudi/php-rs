//! Reflection Extension - PHP Reflection API Implementation
//!
//! Reference: $PHP_SRC_PATH/ext/reflection/
//! Reference: $PHP_SRC_PATH/Zend/zend_reflection.c

use crate::core::value::{ArrayData, ArrayKey, Handle, Symbol, Val, Visibility};
use crate::runtime::context::{ClassDef, MethodEntry, ParameterInfo, RequestContext, TypeHint};
use crate::vm::engine::VM;
use crate::vm::object_helpers::create_object_with_properties;
use std::collections::HashMap;
use std::rc::Rc;

//=============================================================================
// Reflection Internal Data Structures
//=============================================================================

/// Unified parameter representation for both functions and methods
#[derive(Debug, Clone)]
struct UnifiedParam {
    name: Symbol,
    type_hint: Option<TypeHint>,
    is_reference: bool,
    is_variadic: bool,
    default_value: Option<Val>,
}

impl UnifiedParam {
    fn from_parameter_info(param: &ParameterInfo) -> Self {
        Self {
            name: param.name,
            type_hint: param.type_hint.clone(),
            is_reference: param.is_reference,
            is_variadic: param.is_variadic,
            default_value: param.default_value.clone(),
        }
    }
    
    fn from_func_param(param: &crate::compiler::chunk::FuncParam) -> Self {
        Self {
            name: param.name,
            type_hint: param.param_type.as_ref().map(|rt| convert_return_type_to_type_hint(rt)),
            is_reference: param.by_ref,
            is_variadic: param.is_variadic,
            default_value: param.default_value.clone(),
        }
    }
}

/// Convert ReturnType to TypeHint (simplified conversion)
fn convert_return_type_to_type_hint(rt: &crate::compiler::chunk::ReturnType) -> TypeHint {
    use crate::compiler::chunk::ReturnType;
    match rt {
        ReturnType::Int => TypeHint::Int,
        ReturnType::Float => TypeHint::Float,
        ReturnType::String => TypeHint::String,
        ReturnType::Bool => TypeHint::Bool,
        ReturnType::Array => TypeHint::Array,
        ReturnType::Object => TypeHint::Object,
        ReturnType::Void => TypeHint::Void,
        ReturnType::Never => TypeHint::Never,
        ReturnType::Mixed => TypeHint::Mixed,
        ReturnType::Null => TypeHint::Null,
        ReturnType::Callable => TypeHint::Callable,
        ReturnType::Iterable => TypeHint::Iterable,
        ReturnType::Named(sym) => TypeHint::Class(*sym),
        ReturnType::Union(types) => {
            TypeHint::Union(types.iter().map(convert_return_type_to_type_hint).collect())
        }
        ReturnType::Intersection(types) => {
            TypeHint::Intersection(types.iter().map(convert_return_type_to_type_hint).collect())
        }
        ReturnType::Nullable(inner) => {
            // Represent as Union with Null
            TypeHint::Union(vec![convert_return_type_to_type_hint(inner), TypeHint::Null])
        }
        ReturnType::Static | ReturnType::True | ReturnType::False => TypeHint::Mixed,
    }
}

/// Internal data stored in ReflectionClass objects
#[derive(Debug, Clone)]
struct ReflectionClassData {
    class_name: Symbol,
}

/// Internal data stored in ReflectionFunction objects
#[derive(Debug, Clone)]
struct ReflectionFunctionData {
    function_name: Symbol,
}

/// Internal data stored in ReflectionMethod objects
#[derive(Debug, Clone)]
struct ReflectionMethodData {
    class_name: Symbol,
    method_name: Symbol,
}

/// Internal data stored in ReflectionParameter objects
#[derive(Debug, Clone)]
struct ReflectionParameterData {
    function_name: Symbol,
    param_name: Symbol,
    param_index: usize,
}

/// Internal data stored in ReflectionProperty objects
#[derive(Debug, Clone)]
struct ReflectionPropertyData {
    class_name: Symbol,
    property_name: Symbol,
}

//=============================================================================
// Helper Functions
//=============================================================================

/// Get class definition by name from VM context
fn get_class_def(vm: &VM, class_name: Symbol) -> Result<ClassDef, String> {
    vm.context
        .classes
        .get(&class_name)
        .cloned()
        .ok_or_else(|| format!("Class does not exist"))
}

/// Get mutable reference to class definition
fn get_class_def_mut(vm: &mut VM, class_name: Symbol) -> Result<&mut ClassDef, String> {
    vm.context
        .classes
        .get_mut(&class_name)
        .ok_or_else(|| format!("Class does not exist"))
}

/// Get method from class definition
fn get_method(class_def: &ClassDef, method_name: Symbol) -> Result<&MethodEntry, String> {
    class_def
        .methods
        .get(&method_name)
        .ok_or_else(|| format!("Method does not exist"))
}

/// Convert visibility to modifier flags
fn visibility_to_modifiers(visibility: Visibility) -> i64 {
    match visibility {
        Visibility::Public => 1,    // IS_PUBLIC
        Visibility::Protected => 2, // IS_PROTECTED
        Visibility::Private => 4,   // IS_PRIVATE
    }
}

/// Get ReflectionMethod internal data
fn get_reflection_method_data(vm: &mut VM) -> Result<ReflectionMethodData, String> {
    let this_handle = vm
        .frames
        .last()
        .and_then(|f| f.this)
        .ok_or("Method called outside object context")?;

    let class_sym = vm.context.interner.intern(b"class");
    let method_sym = vm.context.interner.intern(b"method");
    
    let this_obj_handle = if let Val::Object(h) = vm.arena.get(this_handle).value {
        h
    } else {
        return Err("Invalid ReflectionMethod object".to_string());
    };
    
    if let Val::ObjPayload(obj_data) = &vm.arena.get(this_obj_handle).value {
        let class_name = if let Some(&h) = obj_data.properties.get(&class_sym) {
            if let Val::String(s) = &vm.arena.get(h).value {
                vm.context.interner.intern(s.as_ref())
            } else {
                return Err("Invalid class property".to_string());
            }
        } else {
            return Err("Missing class property".to_string());
        };

        let method_name = if let Some(&h) = obj_data.properties.get(&method_sym) {
            if let Val::String(s) = &vm.arena.get(h).value {
                // Lowercase for method lookup (PHP stores methods in lowercase)
                let lowercase: Vec<u8> = s.iter().map(|b| b.to_ascii_lowercase()).collect();
                vm.context.interner.intern(&lowercase)
            } else {
                return Err("Invalid method property".to_string());
            }
        } else {
            return Err("Missing method property".to_string());
        };

        return Ok(ReflectionMethodData { class_name, method_name });
    }

    Err("Failed to retrieve ReflectionMethod data".to_string())
}

/// Lookup symbol and return bytes (helper to handle Option)
fn lookup_symbol(vm: &VM, sym: Symbol) -> &[u8] {
    vm.context.interner.lookup(sym).unwrap_or(b"")
}

/// Convert type hint to string
fn type_hint_to_string(vm: &VM, type_hint: &Option<TypeHint>) -> String {
    match type_hint {
        None => String::new(),
        Some(TypeHint::Int) => "int".to_string(),
        Some(TypeHint::Float) => "float".to_string(),
        Some(TypeHint::String) => "string".to_string(),
        Some(TypeHint::Bool) => "bool".to_string(),
        Some(TypeHint::Array) => "array".to_string(),
        Some(TypeHint::Object) => "object".to_string(),
        Some(TypeHint::Callable) => "callable".to_string(),
        Some(TypeHint::Iterable) => "iterable".to_string(),
        Some(TypeHint::Mixed) => "mixed".to_string(),
        Some(TypeHint::Void) => "void".to_string(),
        Some(TypeHint::Never) => "never".to_string(),
        Some(TypeHint::Null) => "null".to_string(),
        Some(TypeHint::Class(sym)) => {
            String::from_utf8_lossy(lookup_symbol(vm, *sym)).to_string()
        }
        Some(TypeHint::Union(types)) => {
            let parts: Vec<String> = types.iter().map(|t| type_hint_to_string(vm, &Some(t.clone()))).collect();
            parts.join("|")
        }
        Some(TypeHint::Intersection(types)) => {
            let parts: Vec<String> = types.iter().map(|t| type_hint_to_string(vm, &Some(t.clone()))).collect();
            parts.join("&")
        }
    }
}

//=============================================================================
// ReflectionClass Implementation
//=============================================================================

/// ReflectionClass::__construct(string|object $objectOrClass)
pub fn reflection_class_construct(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("ReflectionClass::__construct() expects exactly 1 argument, 0 given".to_string());
    }

    let this_handle = vm
        .frames
        .last()
        .and_then(|f| f.this)
        .ok_or("ReflectionClass::__construct() called outside object context")?;

    let arg_val = vm.arena.get(args[0]).value.clone();
    
    let class_name_sym = match arg_val {
        Val::String(ref s) => {
            // Class name as string
            vm.context.interner.intern(s.as_ref())
        }
        Val::Object(payload_handle) => {
            // Object instance - get its class name
            if let Val::ObjPayload(obj_data) = &vm.arena.get(payload_handle).value {
                obj_data.class
            } else {
                return Err("Invalid object payload".to_string());
            }
        }
        _ => {
            return Err("ReflectionClass::__construct() expects parameter 1 to be string or object".to_string());
        }
    };

    // Verify class exists
    if !vm.context.classes.contains_key(&class_name_sym) {
        let class_name_str = String::from_utf8_lossy(lookup_symbol(vm, class_name_sym));
        return Err(format!("Class \"{}\" does not exist", class_name_str));
    }

    // Store class name in object's internal data
    let this_obj_handle = if let Val::Object(h) = vm.arena.get(this_handle).value {
        h
    } else {
        return Err("Invalid ReflectionClass object".to_string());
    };
    
    // Store as a property called "name"
    let name_sym = vm.context.interner.intern(b"name");
    let class_name_bytes = lookup_symbol(vm, class_name_sym).to_vec();
    let name_handle = vm.arena.alloc(Val::String(Rc::new(class_name_bytes)));
    
    if let Val::ObjPayload(obj_data) = &mut vm.arena.get_mut(this_obj_handle).value {
        obj_data.properties.insert(name_sym, name_handle);
    }

    Ok(vm.arena.alloc(Val::Null))
}

/// ReflectionClass::getName(): string
pub fn reflection_class_get_name(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let this_handle = vm
        .frames
        .last()
        .and_then(|f| f.this)
        .ok_or("ReflectionClass::getName() called outside object context")?;

    let name_sym = vm.context.interner.intern(b"name");
    
    let this_obj_handle = if let Val::Object(h) = vm.arena.get(this_handle).value {
        h
    } else {
        return Err("Invalid ReflectionClass object".to_string());
    };
    
    if let Val::ObjPayload(obj_data) = &vm.arena.get(this_obj_handle).value {
        if let Some(&name_handle) = obj_data.properties.get(&name_sym) {
            return Ok(name_handle);
        }
    }

    Err("ReflectionClass::getName() failed to retrieve class name".to_string())
}

/// ReflectionClass::isAbstract(): bool
pub fn reflection_class_is_abstract(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let class_name = get_reflection_class_name(vm)?;
    let class_def = get_class_def(vm, class_name)?;
    Ok(vm.arena.alloc(Val::Bool(class_def.is_abstract)))
}

/// ReflectionClass::isFinal(): bool
pub fn reflection_class_is_final(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let class_name = get_reflection_class_name(vm)?;
    let class_def = get_class_def(vm, class_name)?;

    Ok(vm.arena.alloc(Val::Bool(class_def.is_final)))
}

/// ReflectionClass::isInterface(): bool
pub fn reflection_class_is_interface(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let class_name = get_reflection_class_name(vm)?;
    let class_def = get_class_def(vm, class_name)?;
    Ok(vm.arena.alloc(Val::Bool(class_def.is_interface)))
}

/// ReflectionClass::isTrait(): bool
pub fn reflection_class_is_trait(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let class_name = get_reflection_class_name(vm)?;
    let class_def = get_class_def(vm, class_name)?;
    Ok(vm.arena.alloc(Val::Bool(class_def.is_trait)))
}

/// ReflectionClass::isEnum(): bool
pub fn reflection_class_is_enum(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let class_name = get_reflection_class_name(vm)?;
    let class_def = get_class_def(vm, class_name)?;
    Ok(vm.arena.alloc(Val::Bool(class_def.is_enum)))
}

/// ReflectionClass::isInstantiable(): bool
pub fn reflection_class_is_instantiable(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let class_name = get_reflection_class_name(vm)?;
    let class_def = get_class_def(vm, class_name)?;
    
    // Cannot instantiate if abstract or interface or trait
    let instantiable = !class_def.is_abstract && !class_def.is_interface && !class_def.is_trait;
    Ok(vm.arena.alloc(Val::Bool(instantiable)))
}

/// ReflectionClass::hasMethod(string $name): bool
pub fn reflection_class_has_method(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("ReflectionClass::hasMethod() expects exactly 1 argument, 0 given".to_string());
    }

    let class_name = get_reflection_class_name(vm)?;
    let class_def = get_class_def(vm, class_name)?;
    
    let method_name_val = vm.arena.get(args[0]).value.clone();
    let method_name_bytes = match method_name_val {
        Val::String(ref s) => s.as_ref(),
        _ => return Err("ReflectionClass::hasMethod() expects parameter 1 to be string".to_string()),
    };
    
    let method_sym = vm.context.interner.intern(method_name_bytes);
    let has_method = class_def.methods.contains_key(&method_sym);
    
    Ok(vm.arena.alloc(Val::Bool(has_method)))
}

/// ReflectionClass::hasProperty(string $name): bool
pub fn reflection_class_has_property(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("ReflectionClass::hasProperty() expects exactly 1 argument, 0 given".to_string());
    }

    let class_name = get_reflection_class_name(vm)?;
    let class_def = get_class_def(vm, class_name)?;
    
    let prop_name_val = vm.arena.get(args[0]).value.clone();
    let prop_name_bytes = match prop_name_val {
        Val::String(ref s) => s.as_ref(),
        _ => return Err("ReflectionClass::hasProperty() expects parameter 1 to be string".to_string()),
    };
    
    let prop_sym = vm.context.interner.intern(prop_name_bytes);
    let has_property = class_def.properties.contains_key(&prop_sym) || 
                       class_def.static_properties.contains_key(&prop_sym);
    
    Ok(vm.arena.alloc(Val::Bool(has_property)))
}

/// ReflectionClass::hasConstant(string $name): bool
pub fn reflection_class_has_constant(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("ReflectionClass::hasConstant() expects exactly 1 argument, 0 given".to_string());
    }

    let class_name = get_reflection_class_name(vm)?;
    let class_def = get_class_def(vm, class_name)?;
    
    let const_name_val = vm.arena.get(args[0]).value.clone();
    let const_name_bytes = match const_name_val {
        Val::String(ref s) => s.as_ref(),
        _ => return Err("ReflectionClass::hasConstant() expects parameter 1 to be string".to_string()),
    };
    
    let const_sym = vm.context.interner.intern(const_name_bytes);
    let has_constant = class_def.constants.contains_key(&const_sym);
    
    Ok(vm.arena.alloc(Val::Bool(has_constant)))
}

/// ReflectionClass::getMethods(?int $filter = null): array
pub fn reflection_class_get_methods(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let class_name = get_reflection_class_name(vm)?;
    let class_def = get_class_def(vm, class_name)?;
    
    let mut result = ArrayData::new();
    
    for (method_name_sym, _method_entry) in &class_def.methods {
        let method_name_bytes = lookup_symbol(vm, *method_name_sym).to_vec();
        result.push(vm.arena.alloc(Val::String(Rc::new(method_name_bytes))));
    }
    
    Ok(vm.arena.alloc(Val::Array(Rc::new(result))))
}

/// ReflectionClass::getProperties(?int $filter = null): array
pub fn reflection_class_get_properties(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let class_name = get_reflection_class_name(vm)?;
    let class_def = get_class_def(vm, class_name)?;
    
    let mut result = ArrayData::new();
    
    for (prop_name_sym, _prop_entry) in &class_def.properties {
        let prop_name_bytes = lookup_symbol(vm, *prop_name_sym).to_vec();
        result.push(vm.arena.alloc(Val::String(Rc::new(prop_name_bytes))));
    }
    
    Ok(vm.arena.alloc(Val::Array(Rc::new(result))))
}

/// ReflectionClass::getConstants(): array
pub fn reflection_class_get_constants(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let class_name = get_reflection_class_name(vm)?;
    let class_def = get_class_def(vm, class_name)?;
    
    let mut result = ArrayData::new();
    
    for (const_name_sym, (const_val, _visibility)) in &class_def.constants {
        let const_name_bytes = lookup_symbol(vm, *const_name_sym).to_vec();
        let key = ArrayKey::Str(Rc::new(const_name_bytes));
        result.insert(key, vm.arena.alloc(const_val.clone()));
    }
    
    Ok(vm.arena.alloc(Val::Array(Rc::new(result))))
}

/// ReflectionClass::getConstant(string $name): mixed
pub fn reflection_class_get_constant(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("ReflectionClass::getConstant() expects exactly 1 argument, 0 given".to_string());
    }

    let class_name = get_reflection_class_name(vm)?;
    let class_def = get_class_def(vm, class_name)?;
    
    let const_name_val = vm.arena.get(args[0]).value.clone();
    let const_name_bytes = match const_name_val {
        Val::String(ref s) => s.as_ref(),
        _ => return Err("ReflectionClass::getConstant() expects parameter 1 to be string".to_string()),
    };
    
    let const_sym = vm.context.interner.intern(const_name_bytes);
    
    if let Some((const_val, _visibility)) = class_def.constants.get(&const_sym) {
        Ok(vm.arena.alloc(const_val.clone()))
    } else {
        Ok(vm.arena.alloc(Val::Bool(false)))
    }
}

/// ReflectionClass::getParentClass(): ReflectionClass|false
pub fn reflection_class_get_parent_class(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let class_name = get_reflection_class_name(vm)?;
    let class_def = get_class_def(vm, class_name)?;
    
    if let Some(parent_sym) = class_def.parent {
        let parent_name = lookup_symbol(vm, parent_sym).to_vec();
        create_object_with_properties(
            vm,
            b"ReflectionClass",
            &[(b"name", Val::String(Rc::new(parent_name)))],
        )
    } else {
        Ok(vm.arena.alloc(Val::Bool(false)))
    }
}

/// ReflectionClass::getInterfaceNames(): array
pub fn reflection_class_get_interface_names(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let class_name = get_reflection_class_name(vm)?;
    let class_def = get_class_def(vm, class_name)?;
    
    let mut result = ArrayData::new();
    
    for interface_sym in &class_def.interfaces {
        let interface_name = lookup_symbol(vm, *interface_sym).to_vec();
        result.push(vm.arena.alloc(Val::String(Rc::new(interface_name))));
    }
    
    Ok(vm.arena.alloc(Val::Array(Rc::new(result))))
}

/// ReflectionClass::implementsInterface(ReflectionClass|string $interface): bool
pub fn reflection_class_implements_interface(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("ReflectionClass::implementsInterface() expects exactly 1 argument, 0 given".to_string());
    }

    let class_name = get_reflection_class_name(vm)?;
    let class_def = get_class_def(vm, class_name)?;
    
    let interface_name_val = vm.arena.get(args[0]).value.clone();
    let interface_name_bytes: Vec<u8> = match interface_name_val {
        Val::String(ref s) => s.as_ref().to_vec(),
        Val::Object(payload_handle) => {
            // ReflectionClass object - extract name
            let name_sym = vm.context.interner.intern(b"name");
            if let Val::ObjPayload(obj_data) = &vm.arena.get(payload_handle).value {
                if let Some(&name_handle) = obj_data.properties.get(&name_sym) {
                    let name_val = vm.arena.get(name_handle).value.clone();
                    if let Val::String(s) = name_val {
                        s.as_ref().to_vec()
                    } else {
                        return Err("Invalid ReflectionClass object".to_string());
                    }
                } else {
                    return Err("Invalid ReflectionClass object".to_string());
                }
            } else {
                return Err("Invalid ReflectionClass object".to_string());
            }
        }
        _ => return Err("ReflectionClass::implementsInterface() expects parameter 1 to be string or ReflectionClass".to_string()),
    };
    
    let interface_sym = vm.context.interner.intern(&interface_name_bytes);
    let implements = class_def.interfaces.contains(&interface_sym);
    
    Ok(vm.arena.alloc(Val::Bool(implements)))
}

/// ReflectionClass::getNamespaceName(): string
pub fn reflection_class_get_namespace_name(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let class_name = get_reflection_class_name(vm)?;
    let class_name_bytes = lookup_symbol(vm, class_name);
    
    // Find last backslash to extract namespace
    if let Some(pos) = class_name_bytes.iter().rposition(|&b| b == b'\\') {
        let namespace = &class_name_bytes[..pos];
        Ok(vm.arena.alloc(Val::String(Rc::new(namespace.to_vec()))))
    } else {
        Ok(vm.arena.alloc(Val::String(Rc::new(Vec::new()))))
    }
}

/// ReflectionClass::getShortName(): string
pub fn reflection_class_get_short_name(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let class_name = get_reflection_class_name(vm)?;
    let class_name_bytes = lookup_symbol(vm, class_name);
    
    // Find last backslash to extract short name
    if let Some(pos) = class_name_bytes.iter().rposition(|&b| b == b'\\') {
        let short_name = &class_name_bytes[pos + 1..];
        Ok(vm.arena.alloc(Val::String(Rc::new(short_name.to_vec()))))
    } else {
        Ok(vm.arena.alloc(Val::String(Rc::new(class_name_bytes.to_vec()))))
    }
}

/// ReflectionClass::inNamespace(): bool
pub fn reflection_class_in_namespace(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let class_name = get_reflection_class_name(vm)?;
    let class_name_bytes = lookup_symbol(vm, class_name);
    
    let has_namespace = class_name_bytes.iter().any(|&b| b == b'\\');
    Ok(vm.arena.alloc(Val::Bool(has_namespace)))
}

/// ReflectionClass::__toString(): string
pub fn reflection_class_to_string(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let class_name = get_reflection_class_name(vm)?;
    let class_name_bytes = lookup_symbol(vm, class_name).to_vec();
    let class_name_str = String::from_utf8_lossy(&class_name_bytes);
    
    let output = format!("Class [ <user> class {} ] {{\n}}\n", class_name_str);
    Ok(vm.arena.alloc(Val::String(Rc::new(output.into_bytes()))))
}

/// ReflectionClass::getConstructor(): ?ReflectionMethod
pub fn reflection_class_get_constructor(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let class_name = get_reflection_class_name(vm)?;
    let class_def = get_class_def(vm, class_name)?;
    
    let constructor_sym = vm.context.interner.intern(b"__construct");
    
    // Check if constructor exists
    if class_def.methods.contains_key(&constructor_sym) {
        // Create ReflectionMethod object with properties
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
    } else {
        Ok(vm.arena.alloc(Val::Null))
    }
}

/// ReflectionClass::getMethod(string $name): ReflectionMethod
pub fn reflection_class_get_method(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("ReflectionClass::getMethod() expects exactly 1 argument, 0 given".to_string());
    }

    let class_name = get_reflection_class_name(vm)?;
    let class_def = get_class_def(vm, class_name)?;
    
    let method_name_val = vm.arena.get(args[0]).value.clone();
    let method_name_bytes = match method_name_val {
        Val::String(ref s) => s.as_ref(),
        _ => return Err("ReflectionClass::getMethod() expects parameter 1 to be string".to_string()),
    };
    
    // Method names are case-insensitive in PHP, stored lowercased
    let method_name_lower: Vec<u8> = method_name_bytes.iter().map(|b| b.to_ascii_lowercase()).collect();
    let method_sym = vm.context.interner.intern(&method_name_lower);
    
    // Check if method exists
    if !class_def.methods.contains_key(&method_sym) {
        let method_name_str = String::from_utf8_lossy(method_name_bytes);
        return Err(format!("Method {}() does not exist", method_name_str));
    }
    
    // Create ReflectionMethod object with properties
    let class_name_bytes = lookup_symbol(vm, class_name).to_vec();
    let method_name_bytes_owned = method_name_bytes.to_vec();
    
    create_object_with_properties(
        vm,
        b"ReflectionMethod",
        &[
            (b"class", Val::String(Rc::new(class_name_bytes))),
            (b"method", Val::String(Rc::new(method_name_bytes_owned))),
        ],
    )
}

/// ReflectionClass::getProperty(string $name): ReflectionProperty
pub fn reflection_class_get_property(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("ReflectionClass::getProperty() expects exactly 1 argument, 0 given".to_string());
    }

    let class_name = get_reflection_class_name(vm)?;
    let class_def = get_class_def(vm, class_name)?;
    
    let property_name_val = vm.arena.get(args[0]).value.clone();
    let property_name_bytes = match property_name_val {
        Val::String(ref s) => s.as_ref(),
        _ => return Err("ReflectionClass::getProperty() expects parameter 1 to be string".to_string()),
    };
    
    let property_sym = vm.context.interner.intern(property_name_bytes);
    
    // Check if property exists (in static_properties or instance properties via lookup)
    let exists = class_def.static_properties.contains_key(&property_sym) ||
                 vm.lookup_property(class_name, property_sym).is_some();
    
    if !exists {
        let property_name_str = String::from_utf8_lossy(property_name_bytes);
        return Err(format!("Property {} does not exist", property_name_str));
    }
    
    // Create ReflectionProperty object with properties
    let class_name_bytes = lookup_symbol(vm, class_name).to_vec();
    let property_name_bytes_owned = property_name_bytes.to_vec();
    
    create_object_with_properties(
        vm,
        b"ReflectionProperty",
        &[
            (b"class", Val::String(Rc::new(class_name_bytes))),
            (b"name", Val::String(Rc::new(property_name_bytes_owned))),
        ],
    )
}

/// ReflectionClass::getModifiers(): int
pub fn reflection_class_get_modifiers(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let class_name = get_reflection_class_name(vm)?;
    let class_def = get_class_def(vm, class_name)?;
    
    let mut modifiers = 0;
    
    // These constants match PHP's ReflectionClass constants
    const IS_EXPLICIT_ABSTRACT: i64 = 64;
    const IS_FINAL: i64 = 32;
    
    if class_def.is_abstract {
        modifiers |= IS_EXPLICIT_ABSTRACT;
    }
    if class_def.is_final {
        modifiers |= IS_FINAL;
    }
    
    Ok(vm.arena.alloc(Val::Int(modifiers)))
}

/// ReflectionClass::isInstance(object $object): bool
pub fn reflection_class_is_instance(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("ReflectionClass::isInstance() expects exactly 1 argument, 0 given".to_string());
    }

    let class_name = get_reflection_class_name(vm)?;
    
    let obj_val = vm.arena.get(args[0]).value.clone();
    
    // Check if argument is an object
    let obj_handle = match obj_val {
        Val::Object(h) => h,
        _ => return Ok(vm.arena.alloc(Val::Bool(false))),
    };
    
    // Get the object's class
    let obj_class_sym = if let Val::ObjPayload(obj_data) = &vm.arena.get(obj_handle).value {
        obj_data.class
    } else {
        return Ok(vm.arena.alloc(Val::Bool(false)));
    };
    
    // Simple check: are they the same class?
    let is_instance = obj_class_sym == class_name;
    
    // NOTE: Complete instanceof behavior requires:
    // 1. Walking parent chain (class_def.parent) recursively
    // 2. Checking if class_name is in obj_class_def.interfaces
    // 3. Recursively checking parent class interfaces
    // See PHP's instanceof implementation in Zend/zend_operators.c
    
    Ok(vm.arena.alloc(Val::Bool(is_instance)))
}

/// ReflectionClass::isSubclassOf(ReflectionClass|string $class): bool
pub fn reflection_class_is_subclass_of(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("ReflectionClass::isSubclassOf() expects exactly 1 argument, 0 given".to_string());
    }

    let class_name = get_reflection_class_name(vm)?;
    let class_def = get_class_def(vm, class_name)?;
    
    let parent_name_val = vm.arena.get(args[0]).value.clone();
    let parent_name_bytes: Vec<u8> = match parent_name_val {
        Val::String(ref s) => s.as_ref().to_vec(),
        Val::Object(payload_handle) => {
            // ReflectionClass object - extract name
            let name_sym = vm.context.interner.intern(b"name");
            if let Val::ObjPayload(obj_data) = &vm.arena.get(payload_handle).value {
                if let Some(&name_handle) = obj_data.properties.get(&name_sym) {
                    let name_val = vm.arena.get(name_handle).value.clone();
                    if let Val::String(s) = name_val {
                        s.as_ref().to_vec()
                    } else {
                        return Err("Invalid ReflectionClass object".to_string());
                    }
                } else {
                    return Err("Invalid ReflectionClass object".to_string());
                }
            } else {
                return Err("Invalid ReflectionClass object".to_string());
            }
        }
        _ => return Err("ReflectionClass::isSubclassOf() expects parameter 1 to be string or ReflectionClass".to_string()),
    };
    
    let parent_sym = vm.context.interner.intern(&parent_name_bytes);
    
    // Check if parent_sym is in the parent chain
    if let Some(parent) = class_def.parent {
        if parent == parent_sym {
            return Ok(vm.arena.alloc(Val::Bool(true)));
        }
        // NOTE: Need to recursively check parent's parent for multi-level inheritance:
        // let mut current = parent;
        // while let Some(parent_def) = get_class_def(vm, current).ok() {
        //     if let Some(grandparent) = parent_def.parent {
        //         if grandparent == parent_sym { return true; }
        //         current = grandparent;
        //     } else { break; }
        // }
    }
    
    Ok(vm.arena.alloc(Val::Bool(false)))
}

/// ReflectionClass::newInstance(...$args): object
pub fn reflection_class_new_instance(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // NOTE: Implementation requires:
    // 1. Get class name from ReflectionClass object
    // 2. Create new object instance with create_object_with_properties
    // 3. Look up __construct method if it exists
    // 4. Call constructor with provided args (variadic)
    // 5. Return the initialized object
    // Similar to VM's new_object opcode but driven by reflection
    Ok(vm.arena.alloc(Val::Null))
}

/// ReflectionClass::newInstanceArgs(array $args): object
pub fn reflection_class_new_instance_args(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("ReflectionClass::newInstanceArgs() expects exactly 1 argument, 0 given".to_string());
    }
    // NOTE: Implementation similar to newInstance but:
    // 1. Extract array argument and convert to Vec<Handle>
    // 2. Pass unpacked args to constructor
    // See PHP's reflection_class_new_instance_args in ext/reflection/php_reflection.c
    Ok(vm.arena.alloc(Val::Null))
}

/// ReflectionClass::newInstanceWithoutConstructor(): object
pub fn reflection_class_new_instance_without_constructor(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // NOTE: Implementation:
    // 1. Get class name from ReflectionClass object
    // 2. Create object with create_object_with_properties but skip __construct call
    // 3. Initialize properties with their default values
    // Used for unserialization and testing - bypasses normal construction
    Ok(vm.arena.alloc(Val::Null))
}

/// ReflectionClass::isAnonymous(): bool
pub fn reflection_class_is_anonymous(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let class_name = get_reflection_class_name(vm)?;
    let class_name_bytes = lookup_symbol(vm, class_name);
    
    // Anonymous classes typically contain "class@anonymous" in their name
    let is_anon = class_name_bytes.windows(b"@anonymous".len())
        .any(|w| w == b"@anonymous");
    
    Ok(vm.arena.alloc(Val::Bool(is_anon)))
}

/// ReflectionClass::isCloneable(): bool
pub fn reflection_class_is_cloneable(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let class_name = get_reflection_class_name(vm)?;
    let class_def = get_class_def(vm, class_name)?;
    
    // Class is cloneable if not abstract/interface/trait
    let is_cloneable = !class_def.is_abstract && 
                       !class_def.is_interface &&
                       !class_def.is_trait;
    
    Ok(vm.arena.alloc(Val::Bool(is_cloneable)))
}

/// ReflectionClass::isInternal(): bool
pub fn reflection_class_is_internal(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let class_name = get_reflection_class_name(vm)?;
    
    // Class is internal if NOT in user-defined classes
    let is_internal = !vm.context.classes.contains_key(&class_name);
    
    Ok(vm.arena.alloc(Val::Bool(is_internal)))
}

/// ReflectionClass::isUserDefined(): bool
pub fn reflection_class_is_user_defined(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let class_name = get_reflection_class_name(vm)?;
    
    // Class is user-defined if in classes map
    let is_user_defined = vm.context.classes.contains_key(&class_name);
    
    Ok(vm.arena.alloc(Val::Bool(is_user_defined)))
}

/// ReflectionClass::isIterable(): bool
pub fn reflection_class_is_iterable(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let class_name = get_reflection_class_name(vm)?;
    let class_def = get_class_def(vm, class_name)?;
    
    // Check if class implements Traversable, Iterator, or IteratorAggregate
    let traversable_sym = vm.context.interner.intern(b"Traversable");
    let iterator_sym = vm.context.interner.intern(b"Iterator");
    let iterator_aggregate_sym = vm.context.interner.intern(b"IteratorAggregate");
    
    let is_iterable = class_def.interfaces.contains(&traversable_sym) ||
                      class_def.interfaces.contains(&iterator_sym) ||
                      class_def.interfaces.contains(&iterator_aggregate_sym);
    
    Ok(vm.arena.alloc(Val::Bool(is_iterable)))
}

/// ReflectionClass::getAttributes(): array
pub fn reflection_class_get_attributes(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // NOTE: Attribute reflection (PHP 8.0+) requires:
    // 1. Add attributes: Vec<Attribute> field to ClassDef
    // 2. Parse #[Attribute] syntax in src/parser/class.rs
    // 3. Store attribute name, arguments, and target flags
    // 4. Return array of ReflectionAttribute objects
    // See PHP's reflection_class_get_attributes in ext/reflection/php_reflection.c
    Ok(vm.arena.alloc(Val::Array(Rc::new(ArrayData::new()))))
}

/// ReflectionClass::getDefaultProperties(): array
pub fn reflection_class_get_default_properties(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let class_name = get_reflection_class_name(vm)?;
    let class_def = get_class_def(vm, class_name)?;
    
    let mut result = ArrayData::new();
    
    // Add static properties with their default values
    for (prop_sym, static_prop) in &class_def.static_properties {
        let prop_name_bytes = lookup_symbol(vm, *prop_sym).to_vec();
        let key = ArrayKey::Str(Rc::new(prop_name_bytes));
        result.insert(key, vm.arena.alloc(static_prop.value.clone()));
    }
    
    // Instance properties don't have default values tracked
    
    Ok(vm.arena.alloc(Val::Array(Rc::new(result))))
}

/// ReflectionClass::getDocComment(): string|false
pub fn reflection_class_get_doc_comment(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // NOTE: Doc comment tracking requires:
    // 1. Add doc_comment: Option<String> field to ClassDef
    // 2. Capture /** */ comments before class declarations in parser
    // 3. Associate comment with the following declaration
    // 4. Return comment string or false if not present
    Ok(vm.arena.alloc(Val::Bool(false)))
}

/// ReflectionClass::getFileName(): string|false
pub fn reflection_class_get_file_name(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // NOTE: File name tracking requires:
    // 1. Add file_name: Option<PathBuf> field to ClassDef
    // 2. Pass source file path through parser/compiler pipeline
    // 3. Store in ClassDef during class registration
    // 4. Return absolute path or false for internal classes
    Ok(vm.arena.alloc(Val::Bool(false)))
}

/// ReflectionClass::getStartLine(): int|false
pub fn reflection_class_get_start_line(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // NOTE: Line tracking requires:
    // 1. Add start_line: Option<usize> field to ClassDef
    // 2. Store line number from lexer when parsing class declarations
    // 3. Return line number or false for internal classes
    Ok(vm.arena.alloc(Val::Bool(false)))
}

/// ReflectionClass::getEndLine(): int|false
pub fn reflection_class_get_end_line(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // NOTE: End line tracking requires end_line: Option<usize> in ClassDef
    // Store from lexer when class closing brace is parsed
    Ok(vm.arena.alloc(Val::Bool(false)))
}

/// ReflectionClass::getInterfaces(): array
pub fn reflection_class_get_interfaces(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let class_name = get_reflection_class_name(vm)?;
    let class_def = get_class_def(vm, class_name)?;
    
    let mut result = ArrayData::new();
    
    for interface_sym in &class_def.interfaces {
        let interface_name = lookup_symbol(vm, *interface_sym).to_vec();
        let key = ArrayKey::Str(Rc::new(interface_name.clone()));
        let reflection_obj = create_object_with_properties(
            vm,
            b"ReflectionClass",
            &[(b"name", Val::String(Rc::new(interface_name)))],
        )?;
        result.insert(key, reflection_obj);
    }
    
    Ok(vm.arena.alloc(Val::Array(Rc::new(result))))
}

/// ReflectionClass::getStaticProperties(): array
pub fn reflection_class_get_static_properties(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let class_name = get_reflection_class_name(vm)?;
    let class_def = get_class_def(vm, class_name)?;
    
    let mut result = ArrayData::new();
    
    for (prop_sym, static_prop) in &class_def.static_properties {
        let prop_name_bytes = lookup_symbol(vm, *prop_sym).to_vec();
        let key = ArrayKey::Str(Rc::new(prop_name_bytes));
        result.insert(key, vm.arena.alloc(static_prop.value.clone()));
    }
    
    Ok(vm.arena.alloc(Val::Array(Rc::new(result))))
}

/// ReflectionClass::getStaticPropertyValue(string $name, mixed $default = null): mixed
pub fn reflection_class_get_static_property_value(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("ReflectionClass::getStaticPropertyValue() expects at least 1 argument, 0 given".to_string());
    }

    let class_name = get_reflection_class_name(vm)?;
    let class_def = get_class_def(vm, class_name)?;
    
    let prop_name_val = vm.arena.get(args[0]).value.clone();
    let prop_name_bytes = match prop_name_val {
        Val::String(ref s) => s.as_ref(),
        _ => return Err("ReflectionClass::getStaticPropertyValue() expects parameter 1 to be string".to_string()),
    };
    
    let prop_sym = vm.context.interner.intern(prop_name_bytes);
    
    if let Some(static_prop) = class_def.static_properties.get(&prop_sym) {
        Ok(vm.arena.alloc(static_prop.value.clone()))
    } else {
        // Return default value if provided
        if args.len() >= 2 {
            Ok(args[1])
        } else {
            let prop_name_str = String::from_utf8_lossy(prop_name_bytes);
            Err(format!("Static property {} does not exist", prop_name_str))
        }
    }
}

/// ReflectionClass::setStaticPropertyValue(string $name, mixed $value): void
pub fn reflection_class_set_static_property_value(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() < 2 {
        return Err("ReflectionClass::setStaticPropertyValue() expects exactly 2 arguments".to_string());
    }

    let class_name = get_reflection_class_name(vm)?;
    
    // Extract and clone values before mutating
    let prop_name_val = vm.arena.get(args[0]).value.clone();
    let prop_name_bytes = match prop_name_val {
        Val::String(ref s) => s.as_ref().to_vec(),
        _ => return Err("ReflectionClass::setStaticPropertyValue() expects parameter 1 to be string".to_string()),
    };
    
    let new_value = vm.arena.get(args[1]).value.clone();
    let prop_sym = vm.context.interner.intern(&prop_name_bytes);
    
    // Now get mutable access to class_def
    let class_def = get_class_def_mut(vm, class_name)?;
    
    if let Some(static_prop) = class_def.static_properties.get_mut(&prop_sym) {
        static_prop.value = new_value;
        Ok(vm.arena.alloc(Val::Null))
    } else {
        let prop_name_str = String::from_utf8_lossy(&prop_name_bytes);
        Err(format!("Static property {} does not exist", prop_name_str))
    }
}

/// ReflectionClass::getTraitNames(): array
pub fn reflection_class_get_trait_names(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let class_name = get_reflection_class_name(vm)?;
    let class_def = get_class_def(vm, class_name)?;
    
    let mut result = ArrayData::new();
    
    for trait_sym in &class_def.traits {
        let trait_name = lookup_symbol(vm, *trait_sym).to_vec();
        result.push(vm.arena.alloc(Val::String(Rc::new(trait_name))));
    }
    
    Ok(vm.arena.alloc(Val::Array(Rc::new(result))))
}

/// ReflectionClass::getTraits(): array
pub fn reflection_class_get_traits(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let class_name = get_reflection_class_name(vm)?;
    let class_def = get_class_def(vm, class_name)?;
    
    let mut result = ArrayData::new();
    
    for trait_sym in &class_def.traits {
        let trait_name = lookup_symbol(vm, *trait_sym).to_vec();
        let key = ArrayKey::Str(Rc::new(trait_name.clone()));
        let reflection_obj = create_object_with_properties(
            vm,
            b"ReflectionClass",
            &[(b"name", Val::String(Rc::new(trait_name)))],
        )?;
        result.insert(key, reflection_obj);
    }
    
    Ok(vm.arena.alloc(Val::Array(Rc::new(result))))
}

/// ReflectionClass::getTraitAliases(): array
pub fn reflection_class_get_trait_aliases(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // NOTE: Trait alias tracking requires:
    // 1. Add trait_aliases: HashMap<Symbol, TraitAliasInfo> to ClassDef
    // 2. Parse 'use TraitName { method as alias; }' syntax
    // 3. Store original method name, alias, and visibility changes
    // 4. Return assoc array: ['alias' => 'Trait::method']
    Ok(vm.arena.alloc(Val::Array(Rc::new(ArrayData::new()))))
}

/// ReflectionClass::isReadOnly(): bool
pub fn reflection_class_is_readonly(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // NOTE: Readonly class support (PHP 8.2+) requires:
    // 1. Add is_readonly: bool field to ClassDef
    // 2. Parse 'readonly class Foo' syntax in parser
    // 3. Enforce readonly semantics: all properties must be readonly
    Ok(vm.arena.alloc(Val::Bool(false)))
}

/// ReflectionClass::getReflectionConstant(string $name): ReflectionClassConstant|false
pub fn reflection_class_get_reflection_constant(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("ReflectionClass::getReflectionConstant() expects exactly 1 argument, 0 given".to_string());
    }

    let class_name = get_reflection_class_name(vm)?;
    let class_def = get_class_def(vm, class_name)?;
    
    let const_name_val = vm.arena.get(args[0]).value.clone();
    let const_name_bytes = match const_name_val {
        Val::String(ref s) => s.as_ref(),
        _ => return Err("ReflectionClass::getReflectionConstant() expects parameter 1 to be string".to_string()),
    };
    
    let const_sym = vm.context.interner.intern(const_name_bytes);
    
    if class_def.constants.contains_key(&const_sym) {
        let class_name_bytes = lookup_symbol(vm, class_name).to_vec();
        create_object_with_properties(
            vm,
            b"ReflectionClassConstant",
            &[
                (b"class", Val::String(Rc::new(class_name_bytes))),
                (b"name", Val::String(Rc::new(const_name_bytes.to_vec()))),
            ],
        )
    } else {
        Ok(vm.arena.alloc(Val::Bool(false)))
    }
}

/// ReflectionClass::getReflectionConstants(): array
pub fn reflection_class_get_reflection_constants(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let class_name = get_reflection_class_name(vm)?;
    let class_def = get_class_def(vm, class_name)?;
    let class_name_bytes = lookup_symbol(vm, class_name).to_vec();
    
    let mut result = ArrayData::new();
    
    for (const_sym, _) in &class_def.constants {
        let const_name = lookup_symbol(vm, *const_sym).to_vec();
        let reflection_obj = create_object_with_properties(
            vm,
            b"ReflectionClassConstant",
            &[
                (b"class", Val::String(Rc::new(class_name_bytes.clone()))),
                (b"name", Val::String(Rc::new(const_name))),
            ],
        )?;
        result.push(reflection_obj);
    }
    
    Ok(vm.arena.alloc(Val::Array(Rc::new(result))))
}

/// ReflectionClass::getExtension(): ?ReflectionExtension
pub fn reflection_class_get_extension(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // NOTE: Extension tracking requires:
    // 1. Add extension_name: Option<Symbol> field to ClassDef
    // 2. Set during class registration for built-in classes
    // 3. Return ReflectionExtension object or null for user classes
    Ok(vm.arena.alloc(Val::Null))
}

/// ReflectionClass::getExtensionName(): string|false
pub fn reflection_class_get_extension_name(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // NOTE: Returns extension name string or false for user-defined classes
    // Requires extension_name field in ClassDef (see getExtension above)
    Ok(vm.arena.alloc(Val::Bool(false)))
}

/// ReflectionClass::isIterateable(): bool (alias for isIterable)
pub fn reflection_class_is_iterateable(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    // This is an alias for isIterable()
    reflection_class_is_iterable(vm, args)
}

/// ReflectionClass::getLazyInitializer(object $object): ?Closure
pub fn reflection_class_get_lazy_initializer(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // NOTE: Lazy object support (PHP 8.4+) requires:
    // 1. LazyObject internal type with initializer closure
    // 2. Flag in ObjectData: is_lazy_ghost or is_lazy_proxy
    // 3. Store initializer closure in object internal data
    // 4. Trigger initialization on first property access
    // See PHP RFC: https://wiki.php.net/rfc/lazy-objects
    Ok(vm.arena.alloc(Val::Null))
}

/// ReflectionClass::initializeLazyObject(object $object): object
pub fn reflection_class_initialize_lazy_object(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("ReflectionClass::initializeLazyObject() expects exactly 1 argument, 0 given".to_string());
    }
    // NOTE: Force initialization of lazy object by calling its initializer
    // Returns the initialized object (same reference, now fully populated)
    Ok(args[0])
}

/// ReflectionClass::isUninitializedLazyObject(object $object): bool
pub fn reflection_class_is_uninitialized_lazy_object(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // NOTE: Check if object is lazy and hasn't been initialized yet
    // Would check ObjectData internal state: lazy_initialized flag
    Ok(vm.arena.alloc(Val::Bool(false)))
}

/// ReflectionClass::markLazyObjectAsInitialized(object $object): void
pub fn reflection_class_mark_lazy_object_as_initialized(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // NOTE: Mark lazy object as initialized without calling initializer
    // Used for manual initialization bypass - sets internal flag
    Ok(vm.arena.alloc(Val::Null))
}

/// ReflectionClass::newLazyGhost(callable $initializer, int $options = 0): object
pub fn reflection_class_new_lazy_ghost(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // NOTE: Create lazy ghost object (initialized in-place on first access):
    // 1. Create uninitialized object of the class
    // 2. Store initializer closure in internal data
    // 3. Mark as lazy_ghost type
    // 4. On first property access, call initializer(object)
    // Ghost: object identity preserved, properties filled in-place
    Ok(vm.arena.alloc(Val::Null))
}

/// ReflectionClass::newLazyProxy(callable $factory, int $options = 0): object
pub fn reflection_class_new_lazy_proxy(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // NOTE: Create lazy proxy object (replaced by real object on access):
    // 1. Create proxy placeholder object
    // 2. Store factory closure in internal data
    // 3. Mark as lazy_proxy type
    // 4. On first access, call factory() -> object and replace proxy
    // Proxy: object identity changes, original proxy replaced
    Ok(vm.arena.alloc(Val::Null))
}

/// ReflectionClass::resetAsLazyGhost(object $object, callable $initializer, int $options = 0): void
pub fn reflection_class_reset_as_lazy_ghost(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // NOTE: Convert existing object to lazy ghost:
    // 1. Clear object's current property values
    // 2. Store new initializer closure
    // 3. Mark as lazy_ghost type
    // Used for object recycling/reset scenarios
    Ok(vm.arena.alloc(Val::Null))
}

/// ReflectionClass::resetAsLazyProxy(object $object, callable $factory, int $options = 0): void
pub fn reflection_class_reset_as_lazy_proxy(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // NOTE: Convert existing object to lazy proxy:
    // 1. Clear object's current state
    // 2. Store factory closure
    // 3. Mark as lazy_proxy type for future replacement
    Ok(vm.arena.alloc(Val::Null))
}

//=============================================================================
// ReflectionObject Implementation (extends ReflectionClass)
//=============================================================================

/// ReflectionObject::__construct(object $object)
/// ReflectionObject is a specialized version of ReflectionClass for object instances
pub fn reflection_object_construct(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("ReflectionObject::__construct() expects exactly 1 argument, 0 given".to_string());
    }

    let this_handle = vm
        .frames
        .last()
        .and_then(|f| f.this)
        .ok_or("ReflectionObject::__construct() called outside object context")?;

    // Get the object argument
    let obj_val = vm.arena.get(args[0]).value.clone();
    
    // Must be an object
    let obj_handle = match obj_val {
        Val::Object(h) => h,
        _ => return Err("ReflectionObject::__construct() expects parameter 1 to be object".to_string()),
    };
    
    // Get the class name from the object
    let class_sym = if let Val::ObjPayload(obj_data) = &vm.arena.get(obj_handle).value {
        obj_data.class
    } else {
        return Err("Invalid object".to_string());
    };
    
    // Store both the class name and the object instance
    let this_obj_handle = if let Val::Object(h) = vm.arena.get(this_handle).value {
        h
    } else {
        return Err("Invalid ReflectionObject object".to_string());
    };
    
    let name_sym = vm.context.interner.intern(b"name");
    let object_sym = vm.context.interner.intern(b"object");
    
    let class_name_bytes = lookup_symbol(vm, class_sym);
    let name_handle = vm.arena.alloc(Val::String(Rc::new(class_name_bytes.to_vec())));
    
    if let Val::ObjPayload(obj_data) = &mut vm.arena.get_mut(this_obj_handle).value {
        obj_data.properties.insert(name_sym, name_handle);
        obj_data.properties.insert(object_sym, args[0]); // Store reference to original object
    }

    Ok(vm.arena.alloc(Val::Null))
}

//=============================================================================
// ReflectionEnum Implementation (extends ReflectionClass)
//=============================================================================

/// ReflectionEnum::__construct(string|object $objectOrClass)
/// ReflectionEnum extends ReflectionClass for enum introspection
pub fn reflection_enum_construct(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("ReflectionEnum::__construct() expects exactly 1 argument, 0 given".to_string());
    }

    // Delegate to ReflectionClass constructor logic
    let this_handle = vm
        .frames
        .last()
        .and_then(|f| f.this)
        .ok_or("ReflectionEnum::__construct() called outside object context")?;

    let arg_val = vm.arena.get(args[0]).value.clone();
    
    let class_sym = match arg_val {
        Val::String(s) => {
            vm.context.interner.intern(s.as_ref())
        }
        Val::Object(obj_handle) => {
            if let Val::ObjPayload(obj_data) = &vm.arena.get(obj_handle).value {
                obj_data.class
            } else {
                return Err("Invalid object".to_string());
            }
        }
        _ => return Err("ReflectionEnum::__construct() expects parameter 1 to be string or object".to_string()),
    };
    
    // Verify it's actually an enum
    if let Some(class_def) = vm.context.classes.get(&class_sym) {
        if !class_def.is_enum {
            let class_name = lookup_symbol(vm, class_sym);
            return Err(format!("Class {} is not an enum", String::from_utf8_lossy(class_name)));
        }
    } else {
        let class_name = lookup_symbol(vm, class_sym);
        return Err(format!("Enum {} does not exist", String::from_utf8_lossy(class_name)));
    }
    
    // Store the class name
    let this_obj_handle = if let Val::Object(h) = vm.arena.get(this_handle).value {
        h
    } else {
        return Err("Invalid ReflectionEnum object".to_string());
    };
    
    let name_sym = vm.context.interner.intern(b"name");
    let class_name_bytes = lookup_symbol(vm, class_sym);
    let name_handle = vm.arena.alloc(Val::String(Rc::new(class_name_bytes.to_vec())));
    
    if let Val::ObjPayload(obj_data) = &mut vm.arena.get_mut(this_obj_handle).value {
        obj_data.properties.insert(name_sym, name_handle);
    }

    Ok(vm.arena.alloc(Val::Null))
}

/// ReflectionEnum::isBacked(): bool
/// Determines if the enum is a backed enum (has scalar values)
pub fn reflection_enum_is_backed(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let class_name = get_reflection_class_name(vm)?;
    let class_def = get_class_def(vm, class_name)?;
    
    // NOTE: Proper backing type detection requires:\n    // 1. Add backing_type: Option<BackingType> to ClassDef where BackingType is enum { Int, String }\n    // 2. Parse ': int' or ': string' after enum name during parsing\n    // 3. Store explicitly rather than inferring from constant values\n    // For now, check if any enum cases have values in constants
    // A backed enum has constants with scalar values
    let has_backing = class_def.constants.values()
        .any(|(val, _)| matches!(val, Val::Int(_) | Val::String(_)));
    
    Ok(vm.arena.alloc(Val::Bool(has_backing)))
}

/// ReflectionEnum::getBackingType(): ?ReflectionType
/// Returns the backing type of a backed enum, or null for unit enums
pub fn reflection_enum_get_backing_type(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let class_name = get_reflection_class_name(vm)?;
    let class_def = get_class_def(vm, class_name)?;
    
    // Infer backing type from first constant value
    for (val, _) in class_def.constants.values() {
        match val {
            Val::Int(_) => {
                return create_object_with_properties(
                    vm,
                    b"ReflectionNamedType",
                    &[
                        (b"name", Val::String(Rc::new(b"int".to_vec()))),
                        (b"allowsNull", Val::Bool(false)),
                        (b"isBuiltin", Val::Bool(true)),
                    ],
                );
            }
            Val::String(_) => {
                return create_object_with_properties(
                    vm,
                    b"ReflectionNamedType",
                    &[
                        (b"name", Val::String(Rc::new(b"string".to_vec()))),
                        (b"allowsNull", Val::Bool(false)),
                        (b"isBuiltin", Val::Bool(true)),
                    ],
                );
            }
            _ => continue,
        }
    }
    
    // No backing type (unit enum)
    Ok(vm.arena.alloc(Val::Null))
}

/// ReflectionEnum::hasCase(string $name): bool
/// Checks if the enum has a specific case
pub fn reflection_enum_has_case(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("ReflectionEnum::hasCase() expects exactly 1 argument".to_string());
    }
    
    let case_name = match &vm.arena.get(args[0]).value {
        Val::String(s) => s.as_ref(),
        _ => return Err("ReflectionEnum::hasCase() expects parameter 1 to be string".to_string()),
    };
    
    let case_sym = vm.context.interner.intern(case_name);
    let class_name = get_reflection_class_name(vm)?;
    let class_def = get_class_def(vm, class_name)?;
    
    let has_case = class_def.constants.contains_key(&case_sym);
    
    Ok(vm.arena.alloc(Val::Bool(has_case)))
}

/// ReflectionEnum::getCase(string $name): ReflectionEnumUnitCase
/// Returns a ReflectionEnumUnitCase for the specified case
pub fn reflection_enum_get_case(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("ReflectionEnum::getCase() expects exactly 1 argument".to_string());
    }
    
    let case_name = match &vm.arena.get(args[0]).value {
        Val::String(s) => s.as_ref(),
        _ => return Err("ReflectionEnum::getCase() expects parameter 1 to be string".to_string()),
    };
    
    let case_name_vec = case_name.to_vec();
    let case_sym = vm.context.interner.intern(case_name);
    let class_name = get_reflection_class_name(vm)?;
    let class_def = get_class_def(vm, class_name)?;
    
    if !class_def.constants.contains_key(&case_sym) {
        return Err(format!("Case {} not found", String::from_utf8_lossy(&case_name_vec)));
    }
    
    let class_name_bytes = lookup_symbol(vm, class_name).to_vec();
    create_object_with_properties(
        vm,
        b"ReflectionEnumUnitCase",
        &[
            (b"class", Val::String(Rc::new(class_name_bytes))),
            (b"name", Val::String(Rc::new(case_name_vec))),
        ],
    )
}

/// ReflectionEnum::getCases(): array
/// Returns an array of all ReflectionEnumUnitCase objects
pub fn reflection_enum_get_cases(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let class_name = get_reflection_class_name(vm)?;
    let class_def = get_class_def(vm, class_name)?;
    
    // Return array of case names
    let mut arr = ArrayData::new();
    for (case_sym, _) in class_def.constants.iter() {
        let case_name_bytes = lookup_symbol(vm, *case_sym);
        let case_name_handle = vm.arena.alloc(Val::String(Rc::new(case_name_bytes.to_vec())));
        arr.push(case_name_handle);
    }
    
    Ok(vm.arena.alloc(Val::Array(Rc::new(arr))))
}

//=============================================================================
// ReflectionEnumUnitCase Implementation (extends ReflectionClassConstant)
//=============================================================================

/// ReflectionEnumUnitCase::__construct(string|object $class, string $constant)
/// Creates reflection for an enum case
pub fn reflection_enum_unit_case_construct(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() < 2 {
        return Err("ReflectionEnumUnitCase::__construct() expects exactly 2 arguments".to_string());
    }

    // Use ReflectionClassConstant constructor logic
    let this_handle = vm
        .frames
        .last()
        .and_then(|f| f.this)
        .ok_or("ReflectionEnumUnitCase::__construct() called outside object context")?;

    let class_arg = vm.arena.get(args[0]).value.clone();
    let constant_name_val = vm.arena.get(args[1]).value.clone();

    let class_sym = match class_arg {
        Val::String(s) => vm.context.interner.intern(s.as_ref()),
        Val::Object(obj_handle) => {
            if let Val::ObjPayload(obj_data) = &vm.arena.get(obj_handle).value {
                obj_data.class
            } else {
                return Err("Invalid object".to_string());
            }
        }
        _ => return Err("ReflectionEnumUnitCase::__construct() expects parameter 1 to be string or object".to_string()),
    };

    let constant_name_bytes = match constant_name_val {
        Val::String(ref s) => s.as_ref(),
        _ => return Err("ReflectionEnumUnitCase::__construct() expects parameter 2 to be string".to_string()),
    };

    // Verify the class is an enum
    if let Some(class_def) = vm.context.classes.get(&class_sym) {
        if !class_def.is_enum {
            let class_name = lookup_symbol(vm, class_sym);
            return Err(format!("Class {} is not an enum", String::from_utf8_lossy(class_name)));
        }
        
        let constant_sym = vm.context.interner.intern(constant_name_bytes);
        if !class_def.constants.contains_key(&constant_sym) {
            return Err(format!("Case {} not found", String::from_utf8_lossy(constant_name_bytes)));
        }
    } else {
        let class_name = lookup_symbol(vm, class_sym);
        return Err(format!("Enum {} does not exist", String::from_utf8_lossy(class_name)));
    }

    // Store class name and constant name
    let this_obj_handle = if let Val::Object(h) = vm.arena.get(this_handle).value {
        h
    } else {
        return Err("Invalid ReflectionEnumUnitCase object".to_string());
    };

    let class_name_sym = vm.context.interner.intern(b"className");
    let constant_name_sym = vm.context.interner.intern(b"constantName");

    let class_name_bytes = lookup_symbol(vm, class_sym);
    let class_name_handle = vm.arena.alloc(Val::String(Rc::new(class_name_bytes.to_vec())));
    let constant_name_handle = vm.arena.alloc(Val::String(Rc::new(constant_name_bytes.to_vec())));

    if let Val::ObjPayload(obj_data) = &mut vm.arena.get_mut(this_obj_handle).value {
        obj_data.properties.insert(class_name_sym, class_name_handle);
        obj_data.properties.insert(constant_name_sym, constant_name_handle);
    }

    Ok(vm.arena.alloc(Val::Null))
}

/// ReflectionEnumUnitCase::getEnum(): ReflectionEnum
/// Gets the reflection of the enum that contains this case
pub fn reflection_enum_unit_case_get_enum(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let data = get_reflection_class_constant_data(vm)?;
    
    let class_name_bytes = lookup_symbol(vm, data.class_name).to_vec();
    
    create_object_with_properties(
        vm,
        b"ReflectionEnum",
        &[(b"name", Val::String(Rc::new(class_name_bytes)))],
    )
}

/// ReflectionEnumUnitCase::getValue(): object
/// Gets the actual enum case object (the enum instance)
pub fn reflection_enum_unit_case_get_value(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let data = get_reflection_class_constant_data(vm)?;
    
    if let Some(class_def) = vm.context.classes.get(&data.class_name) {
        if let Some((val, _visibility)) = class_def.constants.get(&data.constant_name) {
            // Return the enum case value
            // For enums, this would be the enum case object
            // For now, return the constant value
            return Ok(vm.arena.alloc(val.clone()));
        }
    }
    
    Err("Enum case not found".to_string())
}

//=============================================================================
// ReflectionEnumBackedCase Implementation (extends ReflectionEnumUnitCase)
//=============================================================================

/// ReflectionEnumBackedCase::getBackingValue(): int|string
/// Gets the backing/scalar value of a backed enum case
pub fn reflection_enum_backed_case_get_backing_value(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let data = get_reflection_class_constant_data(vm)?;
    
    if let Some(class_def) = vm.context.classes.get(&data.class_name) {
        if !class_def.is_enum {
            return Err("Not an enum".to_string());
        }
        
        if let Some((val, _visibility)) = class_def.constants.get(&data.constant_name) {
            // For backed enums, the case value should be a scalar (int or string)
            // Return the backing value
            match val {
                Val::Int(_) | Val::String(_) => Ok(vm.arena.alloc(val.clone())),
                _ => Err("Enum case does not have a backing value".to_string()),
            }
        } else {
            Err("Enum case not found".to_string())
        }
    } else {
        Err("Enum class not found".to_string())
    }
}

//=============================================================================
// ReflectionExtension Implementation
//=============================================================================

/// Helper struct to hold extension data
#[derive(Debug)]
struct ReflectionExtensionData {
    name: Symbol,
}

/// Extract extension name from ReflectionExtension object
fn get_reflection_extension_data(vm: &mut VM) -> Result<ReflectionExtensionData, String> {
    let this_handle = vm
        .frames
        .last()
        .and_then(|f| f.this)
        .ok_or("ReflectionExtension method called outside object context")?;

    let this_obj_handle = if let Val::Object(h) = vm.arena.get(this_handle).value {
        h
    } else {
        return Err("Invalid ReflectionExtension object".to_string());
    };

    let name_sym = vm.context.interner.intern(b"name");

    if let Val::ObjPayload(obj_data) = &vm.arena.get(this_obj_handle).value {
        if let Some(&name_handle) = obj_data.properties.get(&name_sym) {
            if let Val::String(ref s) = vm.arena.get(name_handle).value {
                let name_symbol = vm.context.interner.intern(s.as_ref());
                return Ok(ReflectionExtensionData {
                    name: name_symbol,
                });
            }
        }
    }

    Err("ReflectionExtension object missing extension name".to_string())
}

/// ReflectionExtension::__construct(string $name)
/// Creates a ReflectionExtension for the specified extension
pub fn reflection_extension_construct(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("ReflectionExtension::__construct() expects exactly 1 argument".to_string());
    }

    let this_handle = vm
        .frames
        .last()
        .and_then(|f| f.this)
        .ok_or("ReflectionExtension::__construct() called outside object context")?;

    let ext_name_val = vm.arena.get(args[0]).value.clone();
    let ext_name_bytes = match ext_name_val {
        Val::String(ref s) => s.as_ref(),
        _ => return Err("ReflectionExtension::__construct() expects parameter 1 to be string".to_string()),
    };

    // For now, accept any extension name (proper validation would check loaded extensions)
    let this_obj_handle = if let Val::Object(h) = vm.arena.get(this_handle).value {
        h
    } else {
        return Err("Invalid ReflectionExtension object".to_string());
    };

    let name_sym = vm.context.interner.intern(b"name");
    let ext_name_handle = vm.arena.alloc(Val::String(Rc::new(ext_name_bytes.to_vec())));

    if let Val::ObjPayload(obj_data) = &mut vm.arena.get_mut(this_obj_handle).value {
        obj_data.properties.insert(name_sym, ext_name_handle);
    }

    Ok(vm.arena.alloc(Val::Null))
}

/// ReflectionExtension::getName(): string
/// Gets the name of the extension
pub fn reflection_extension_get_name(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let data = get_reflection_extension_data(vm)?;
    let name_bytes = lookup_symbol(vm, data.name);
    Ok(vm.arena.alloc(Val::String(Rc::new(name_bytes.to_vec()))))
}

/// ReflectionExtension::getVersion(): ?string
/// Gets the version of the extension
pub fn reflection_extension_get_version(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let _data = get_reflection_extension_data(vm)?;
    // NOTE: Extension version tracking requires:
    // 1. Add version: String field to ExtensionInfo struct
    // 2. Set during extension registration in runtime/extension.rs
    // 3. Store in VM's extension registry
    // 4. Look up by extension name and return version string
    Ok(vm.arena.alloc(Val::Null))
}

/// ReflectionExtension::getFunctions(): array
/// Gets functions provided by the extension
pub fn reflection_extension_get_functions(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let _data = get_reflection_extension_data(vm)?;
    // NOTE: Function-to-extension mapping requires:
    // 1. Add extension_name: Option<Symbol> to function metadata
    // 2. Tag functions during extension registration
    // 3. Add VM method: get_functions_by_extension(name) -> Vec<Symbol>
    // 4. Return array of ReflectionFunction objects
    Ok(vm.arena.alloc(Val::Array(Rc::new(ArrayData::new()))))
}

/// ReflectionExtension::getConstants(): array
/// Gets constants provided by the extension
pub fn reflection_extension_get_constants(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let _data = get_reflection_extension_data(vm)?;
    // NOTE: Constant-to-extension mapping requires:
    // 1. Add extension_name field to constant metadata
    // 2. Track during constant registration
    // 3. Add VM method: get_constants_by_extension(name) -> HashMap<Symbol, Val>
    // 4. Return assoc array ['NAME' => value]
    Ok(vm.arena.alloc(Val::Array(Rc::new(ArrayData::new()))))
}

/// ReflectionExtension::getINIEntries(): array
/// Gets INI entries for the extension
pub fn reflection_extension_get_ini_entries(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let _data = get_reflection_extension_data(vm)?;
    // NOTE: INI entries per extension requires:
    // 1. Extension-specific INI configuration system
    // 2. Map extension name -> INI keys in runtime context
    // 3. Return assoc array ['ini.key' => 'value']
    // Example: ['mysqli.default_port' => '3306']
    Ok(vm.arena.alloc(Val::Array(Rc::new(ArrayData::new()))))
}

/// ReflectionExtension::getClasses(): array
/// Gets classes provided by the extension
pub fn reflection_extension_get_classes(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let _data = get_reflection_extension_data(vm)?;
    // NOTE: Class-to-extension mapping for getClasses() requires:
    // 1. extension_name field in ClassDef (see getExtension above)
    // 2. Filter classes by extension name
    // 3. Return assoc array ['ClassName' => ReflectionClass]
    Ok(vm.arena.alloc(Val::Array(Rc::new(ArrayData::new()))))
}

/// ReflectionExtension::getClassNames(): array
/// Gets names of classes provided by the extension
pub fn reflection_extension_get_class_names(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let _data = get_reflection_extension_data(vm)?;
    // NOTE: Similar to getClasses() but returns array of class name strings
    // Requires same extension_name field in ClassDef
    Ok(vm.arena.alloc(Val::Array(Rc::new(ArrayData::new()))))
}

/// ReflectionExtension::getDependencies(): array
/// Gets dependencies of the extension
pub fn reflection_extension_get_dependencies(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let _data = get_reflection_extension_data(vm)?;
    // NOTE: Extension dependency tracking requires:
    // 1. Add dependencies: Vec<String> to ExtensionInfo
    // 2. Declare during extension registration (e.g., mysqli depends on mysqlnd)
    // 3. Return assoc array ['required' => [...], 'optional' => [...]]
    Ok(vm.arena.alloc(Val::Array(Rc::new(ArrayData::new()))))
}

/// ReflectionExtension::info(): void
/// Prints information about the extension
pub fn reflection_extension_info(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let data = get_reflection_extension_data(vm)?;
    let name_bytes = lookup_symbol(vm, data.name);
    
    // Print basic extension info
    println!("Extension [ {} ] {{", String::from_utf8_lossy(name_bytes));
    println!("  Classes [0] {{");
    println!("  }}");
    println!("  Functions [0] {{");
    println!("  }}");
    println!("  Constants [0] {{");
    println!("  }}");
    println!("  INI entries [0] {{");
    println!("  }}");
    println!("  Dependencies [0] {{");
    println!("  }}");
    println!("}}");
    
    Ok(vm.arena.alloc(Val::Null))
}

/// ReflectionExtension::isPersistent(): bool
/// Checks if the extension is persistent
pub fn reflection_extension_is_persistent(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let _data = get_reflection_extension_data(vm)?;
    // NOTE: Persistent vs temporary extension tracking:
    // 1. Add is_persistent: bool to ExtensionInfo
    // 2. Built-in extensions are persistent (always loaded)
    // 3. Dynamically loaded (dl()) are temporary
    // For now, assume all extensions are persistent
    Ok(vm.arena.alloc(Val::Bool(true)))
}

/// ReflectionExtension::isTemporary(): bool
/// Checks if the extension is temporary
pub fn reflection_extension_is_temporary(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let _data = get_reflection_extension_data(vm)?;
    // NOTE: Inverse of isPersistent() - true for dl() loaded extensions
    Ok(vm.arena.alloc(Val::Bool(false)))
}

//=============================================================================
// Reflection Implementation (Static utility class)
//=============================================================================

/// Reflection::export(Reflector $reflector, bool $return = false): ?string
/// Exports a reflection (deprecated in PHP 8.0, returns null)
pub fn reflection_export(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // This method is deprecated in PHP 8.0 and removed in PHP 8.1
    // Return null to indicate it's not supported
    Ok(vm.arena.alloc(Val::Null))
}

/// Reflection::getModifierNames(int $modifiers): array
/// Returns an array of modifier names from a modifier bitmask
pub fn reflection_get_modifier_names(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("Reflection::getModifierNames() expects exactly 1 argument".to_string());
    }

    let modifiers = match vm.arena.get(args[0]).value {
        Val::Int(i) => i,
        _ => return Err("Reflection::getModifierNames() expects parameter 1 to be int".to_string()),
    };

    let mut names = Vec::new();
    let mut arr_data = ArrayData::new();

    // PHP modifier constants:
    // IS_STATIC = 1, IS_ABSTRACT = 2, IS_FINAL = 4
    // ReflectionClass modifiers: IS_FINAL = 32, IS_EXPLICIT_ABSTRACT = 64
    // IS_PUBLIC = 256, IS_PROTECTED = 512, IS_PRIVATE = 1024
    // IS_READONLY = 2048

    // Check visibility modifiers first
    if modifiers & 256 != 0 {  // IS_PUBLIC
        names.push(b"public".to_vec());
    }
    if modifiers & 512 != 0 {  // IS_PROTECTED
        names.push(b"protected".to_vec());
    }
    if modifiers & 1024 != 0 {  // IS_PRIVATE
        names.push(b"private".to_vec());
    }

    // Check other modifiers
    if modifiers & 1 != 0 {  // IS_STATIC
        names.push(b"static".to_vec());
    }
    if modifiers & 2 != 0 || modifiers & 64 != 0 {  // IS_ABSTRACT / IS_EXPLICIT_ABSTRACT
        names.push(b"abstract".to_vec());
    }
    if modifiers & 4 != 0 || modifiers & 32 != 0 {  // IS_FINAL
        names.push(b"final".to_vec());
    }
    if modifiers & 2048 != 0 {  // IS_READONLY
        names.push(b"readonly".to_vec());
    }

    // Build array
    for (i, name) in names.iter().enumerate() {
        let name_handle = vm.arena.alloc(Val::String(Rc::new(name.clone())));
        arr_data.map.insert(ArrayKey::Int(i as i64), name_handle);
    }

    Ok(vm.arena.alloc(Val::Array(Rc::new(arr_data))))
}

//=============================================================================
// ReflectionReference Implementation
//=============================================================================

/// ReflectionReference::fromArrayElement(array $array, int|string $key): ?ReflectionReference
/// Creates a ReflectionReference from an array element (static method)
pub fn reflection_reference_from_array_element(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() < 2 {
        return Err("ReflectionReference::fromArrayElement() expects exactly 2 arguments".to_string());
    }

    // NOTE: Reference tracking infrastructure requires:
    // 1. Add reference ID/counter to Val enum or separate reference table
    // 2. Track which values are references vs copies
    // 3. Assign unique IDs to reference groups
    // 4. Check if array[key] is a reference and return ReflectionReference or null
    // See PHP's ZEND_ISREF() macro and zval reference counting
    Ok(vm.arena.alloc(Val::Null))
}

/// ReflectionReference::getId(): string
/// Gets a unique identifier for the reference
pub fn reflection_reference_get_id(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // NOTE: Returns unique string ID for reference group (e.g., "0x7f8b8c0")
    // All variables that reference the same value share the same ID
    // Requires reference tracking infrastructure (see fromArrayElement)
    Ok(vm.arena.alloc(Val::String(Rc::new(b"ref_placeholder".to_vec()))))
}

//=============================================================================
// ReflectionZendExtension Implementation
//=============================================================================

/// Helper struct to store ReflectionZendExtension data
struct ReflectionZendExtensionData {
    name: Symbol,
}

/// Helper function to get ReflectionZendExtension data from an object
fn get_reflection_zend_extension_data(vm: &mut VM) -> Result<ReflectionZendExtensionData, String> {
    let this_handle = vm
        .frames
        .last()
        .and_then(|f| f.this)
        .ok_or("ReflectionZendExtension method called outside object context")?;

    let this_obj_handle = if let Val::Object(h) = vm.arena.get(this_handle).value {
        h
    } else {
        return Err("Invalid ReflectionZendExtension object".to_string());
    };

    let name_sym = vm.context.interner.intern(b"name");

    if let Val::ObjPayload(obj_data) = &vm.arena.get(this_obj_handle).value {
        if let Some(&name_handle) = obj_data.properties.get(&name_sym) {
            if let Val::String(ref s) = vm.arena.get(name_handle).value {
                let name_symbol = vm.context.interner.intern(s.as_ref());
                return Ok(ReflectionZendExtensionData {
                    name: name_symbol,
                });
            }
        }
    }

    Err("ReflectionZendExtension object missing extension name".to_string())
}

/// ReflectionZendExtension::__construct(string $name)
pub fn reflection_zend_extension_construct(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("ReflectionZendExtension::__construct() expects exactly 1 argument".to_string());
    }

    let this_handle = vm
        .frames
        .last()
        .and_then(|f| f.this)
        .ok_or("ReflectionZendExtension::__construct() called outside object context")?;

    let ext_name_val = vm.arena.get(args[0]).value.clone();
    let ext_name_bytes = match ext_name_val {
        Val::String(ref s) => s.as_ref(),
        _ => return Err("ReflectionZendExtension::__construct() expects parameter 1 to be string".to_string()),
    };

    let this_obj_handle = if let Val::Object(h) = vm.arena.get(this_handle).value {
        h
    } else {
        return Err("Invalid ReflectionZendExtension object".to_string());
    };

    let name_sym = vm.context.interner.intern(b"name");
    let ext_name_handle = vm.arena.alloc(Val::String(Rc::new(ext_name_bytes.to_vec())));

    if let Val::ObjPayload(obj_data) = &mut vm.arena.get_mut(this_obj_handle).value {
        obj_data.properties.insert(name_sym, ext_name_handle);
    }

    Ok(vm.arena.alloc(Val::Null))
}

/// ReflectionZendExtension::getName(): string
pub fn reflection_zend_extension_get_name(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let data = get_reflection_zend_extension_data(vm)?;
    let name_bytes = vm.context.interner.lookup(data.name)
        .ok_or("Failed to lookup extension name symbol")?
        .to_vec();
    Ok(vm.arena.alloc(Val::String(Rc::new(name_bytes))))
}

/// ReflectionZendExtension::getVersion(): string
pub fn reflection_zend_extension_get_version(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let _data = get_reflection_zend_extension_data(vm)?;
    // Stub: No Zend extension version tracking yet
    Ok(vm.arena.alloc(Val::String(Rc::new(b"1.0.0".to_vec()))))
}

/// ReflectionZendExtension::getAuthor(): string
pub fn reflection_zend_extension_get_author(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let _data = get_reflection_zend_extension_data(vm)?;
    // Stub: No Zend extension author tracking yet
    Ok(vm.arena.alloc(Val::String(Rc::new(b"Unknown".to_vec()))))
}

/// ReflectionZendExtension::getURL(): string
pub fn reflection_zend_extension_get_url(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let _data = get_reflection_zend_extension_data(vm)?;
    // Stub: No Zend extension URL tracking yet
    Ok(vm.arena.alloc(Val::String(Rc::new(b"https://example.com".to_vec()))))
}

/// ReflectionZendExtension::getCopyright(): string
pub fn reflection_zend_extension_get_copyright(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let _data = get_reflection_zend_extension_data(vm)?;
    // Stub: No Zend extension copyright tracking yet
    Ok(vm.arena.alloc(Val::String(Rc::new(b"Copyright (c) 2026".to_vec()))))
}

//=============================================================================
// ReflectionGenerator Implementation
//=============================================================================

/// Helper struct to store ReflectionGenerator data
struct ReflectionGeneratorData {
    generator_handle: Handle,
}

/// Helper function to get ReflectionGenerator data from an object
fn get_reflection_generator_data(vm: &mut VM) -> Result<ReflectionGeneratorData, String> {
    let this_handle = vm
        .frames
        .last()
        .and_then(|f| f.this)
        .ok_or("ReflectionGenerator method called outside object context")?;

    let this_obj_handle = if let Val::Object(h) = vm.arena.get(this_handle).value {
        h
    } else {
        return Err("Invalid ReflectionGenerator object".to_string());
    };

    let generator_sym = vm.context.interner.intern(b"generator");

    if let Val::ObjPayload(obj_data) = &vm.arena.get(this_obj_handle).value {
        if let Some(&generator_handle) = obj_data.properties.get(&generator_sym) {
            return Ok(ReflectionGeneratorData { generator_handle });
        }
    }

    Err("ReflectionGenerator object missing generator reference".to_string())
}

/// ReflectionGenerator::__construct(Generator $generator)
pub fn reflection_generator_construct(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("ReflectionGenerator::__construct() expects exactly 1 argument".to_string());
    }

    let this_handle = vm
        .frames
        .last()
        .and_then(|f| f.this)
        .ok_or("ReflectionGenerator::__construct() called outside object context")?;

    let generator_val = vm.arena.get(args[0]).value.clone();

    let this_obj_handle = if let Val::Object(h) = vm.arena.get(this_handle).value {
        h
    } else {
        return Err("Invalid ReflectionGenerator object".to_string());
    };

    let generator_sym = vm.context.interner.intern(b"generator");
    let generator_handle = vm.arena.alloc(generator_val);

    if let Val::ObjPayload(obj_data) = &mut vm.arena.get_mut(this_obj_handle).value {
        obj_data.properties.insert(generator_sym, generator_handle);
    }

    Ok(vm.arena.alloc(Val::Null))
}

/// ReflectionGenerator::getExecutingFile(): string
pub fn reflection_generator_get_executing_file(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let _data = get_reflection_generator_data(vm)?;
    // Stub: Generator execution tracking not implemented
    Ok(vm.arena.alloc(Val::String(Rc::new(b"unknown".to_vec()))))
}

/// ReflectionGenerator::getExecutingLine(): int
pub fn reflection_generator_get_executing_line(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let _data = get_reflection_generator_data(vm)?;
    // Stub: Generator execution tracking not implemented
    Ok(vm.arena.alloc(Val::Int(0)))
}

/// ReflectionGenerator::getExecutingGenerator(): Generator
pub fn reflection_generator_get_executing_generator(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let data = get_reflection_generator_data(vm)?;
    // Return the stored generator reference
    Ok(data.generator_handle)
}

/// ReflectionGenerator::getFunction(): ReflectionFunctionAbstract
pub fn reflection_generator_get_function(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let _data = get_reflection_generator_data(vm)?;
    // Stub: Return null since ReflectionFunctionAbstract not implemented
    Ok(vm.arena.alloc(Val::Null))
}

/// ReflectionGenerator::getThis(): ?object
pub fn reflection_generator_get_this(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let _data = get_reflection_generator_data(vm)?;
    // Stub: Generator $this tracking not implemented
    Ok(vm.arena.alloc(Val::Null))
}

/// ReflectionGenerator::getTrace(int $options = DEBUG_BACKTRACE_PROVIDE_OBJECT): array
pub fn reflection_generator_get_trace(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let _data = get_reflection_generator_data(vm)?;
    // Stub: Generator stack trace not implemented
    Ok(vm.arena.alloc(Val::Array(Rc::new(ArrayData::new()))))
}

/// ReflectionGenerator::isClosed(): bool
pub fn reflection_generator_is_closed(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let _data = get_reflection_generator_data(vm)?;
    // Stub: Generator state tracking not implemented
    // Assume closed for now
    Ok(vm.arena.alloc(Val::Bool(true)))
}

//=============================================================================
// ReflectionFiber Implementation
//=============================================================================

/// Helper struct to store ReflectionFiber data
struct ReflectionFiberData {
    fiber_handle: Handle,
}

/// Helper function to get ReflectionFiber data from an object
fn get_reflection_fiber_data(vm: &mut VM) -> Result<ReflectionFiberData, String> {
    let this_handle = vm
        .frames
        .last()
        .and_then(|f| f.this)
        .ok_or("ReflectionFiber method called outside object context")?;

    let this_obj_handle = if let Val::Object(h) = vm.arena.get(this_handle).value {
        h
    } else {
        return Err("Invalid ReflectionFiber object".to_string());
    };

    let fiber_sym = vm.context.interner.intern(b"fiber");

    if let Val::ObjPayload(obj_data) = &vm.arena.get(this_obj_handle).value {
        if let Some(&fiber_handle) = obj_data.properties.get(&fiber_sym) {
            return Ok(ReflectionFiberData { fiber_handle });
        }
    }

    Err("ReflectionFiber object missing fiber reference".to_string())
}

/// ReflectionFiber::__construct(Fiber $fiber)
pub fn reflection_fiber_construct(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("ReflectionFiber::__construct() expects exactly 1 argument".to_string());
    }

    let this_handle = vm
        .frames
        .last()
        .and_then(|f| f.this)
        .ok_or("ReflectionFiber::__construct() called outside object context")?;

    let fiber_val = vm.arena.get(args[0]).value.clone();

    let this_obj_handle = if let Val::Object(h) = vm.arena.get(this_handle).value {
        h
    } else {
        return Err("Invalid ReflectionFiber object".to_string());
    };

    let fiber_sym = vm.context.interner.intern(b"fiber");
    let fiber_handle = vm.arena.alloc(fiber_val);

    if let Val::ObjPayload(obj_data) = &mut vm.arena.get_mut(this_obj_handle).value {
        obj_data.properties.insert(fiber_sym, fiber_handle);
    }

    Ok(vm.arena.alloc(Val::Null))
}

/// ReflectionFiber::getFiber(): Fiber
pub fn reflection_fiber_get_fiber(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let data = get_reflection_fiber_data(vm)?;
    // Return the stored fiber reference
    Ok(data.fiber_handle)
}

/// ReflectionFiber::getCallable(): callable
pub fn reflection_fiber_get_callable(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let _data = get_reflection_fiber_data(vm)?;
    // Stub: Fiber callable tracking not implemented
    Ok(vm.arena.alloc(Val::Null))
}

/// ReflectionFiber::getExecutingFile(): string
pub fn reflection_fiber_get_executing_file(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let _data = get_reflection_fiber_data(vm)?;
    // Stub: Fiber execution tracking not implemented
    Ok(vm.arena.alloc(Val::String(Rc::new(b"unknown".to_vec()))))
}

/// ReflectionFiber::getExecutingLine(): int
pub fn reflection_fiber_get_executing_line(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let _data = get_reflection_fiber_data(vm)?;
    // Stub: Fiber execution tracking not implemented
    Ok(vm.arena.alloc(Val::Int(0)))
}

/// ReflectionFiber::getTrace(int $options = DEBUG_BACKTRACE_PROVIDE_OBJECT): array
pub fn reflection_fiber_get_trace(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let _data = get_reflection_fiber_data(vm)?;
    // Stub: Fiber stack trace not implemented
    Ok(vm.arena.alloc(Val::Array(Rc::new(ArrayData::new()))))
}

//=============================================================================
// ReflectionFunctionAbstract Implementation
//=============================================================================
// Note: This is an abstract base class in PHP. We register it but it cannot
// be instantiated directly. ReflectionFunction and ReflectionMethod should
// extend this class (inheritance not yet refactored).

/// ReflectionFunctionAbstract::getClosureScopeClass(): ?ReflectionClass
pub fn reflection_function_abstract_get_closure_scope_class(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // Stub: Closure scope tracking not implemented
    Ok(vm.arena.alloc(Val::Null))
}

/// ReflectionFunctionAbstract::getClosureThis(): ?object
pub fn reflection_function_abstract_get_closure_this(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // Stub: Closure $this tracking not implemented
    Ok(vm.arena.alloc(Val::Null))
}

/// ReflectionFunctionAbstract::getClosureUsedVariables(): array
pub fn reflection_function_abstract_get_closure_used_variables(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // Stub: Closure used variables tracking not implemented
    Ok(vm.arena.alloc(Val::Array(Rc::new(ArrayData::new()))))
}

/// ReflectionFunctionAbstract::getDocComment(): string|false
pub fn reflection_function_abstract_get_doc_comment(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // Stub: Doc comment parsing not implemented
    Ok(vm.arena.alloc(Val::Bool(false)))
}

/// ReflectionFunctionAbstract::getEndLine(): int|false
pub fn reflection_function_abstract_get_end_line(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // Stub: Source line tracking not implemented
    Ok(vm.arena.alloc(Val::Bool(false)))
}

/// ReflectionFunctionAbstract::getExtension(): ?ReflectionExtension
pub fn reflection_function_abstract_get_extension(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // Stub: Extension tracking not implemented
    Ok(vm.arena.alloc(Val::Null))
}

/// ReflectionFunctionAbstract::getExtensionName(): string|false
pub fn reflection_function_abstract_get_extension_name(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // Stub: Extension tracking not implemented
    Ok(vm.arena.alloc(Val::Bool(false)))
}

/// ReflectionFunctionAbstract::getReturnType(): ?ReflectionType
pub fn reflection_function_abstract_get_return_type(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // Stub: Return type reflection not fully implemented
    Ok(vm.arena.alloc(Val::Null))
}

/// ReflectionFunctionAbstract::getStartLine(): int|false
pub fn reflection_function_abstract_get_start_line(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // Stub: Source line tracking not implemented
    Ok(vm.arena.alloc(Val::Bool(false)))
}

/// ReflectionFunctionAbstract::getStaticVariables(): array
pub fn reflection_function_abstract_get_static_variables(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // Stub: Static variable tracking not implemented
    Ok(vm.arena.alloc(Val::Array(Rc::new(ArrayData::new()))))
}

/// ReflectionFunctionAbstract::hasReturnType(): bool
pub fn reflection_function_abstract_has_return_type(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // Stub: Return type tracking not implemented
    Ok(vm.arena.alloc(Val::Bool(false)))
}

/// ReflectionFunctionAbstract::isDeprecated(): bool
pub fn reflection_function_abstract_is_deprecated(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // Stub: Deprecation tracking not implemented
    Ok(vm.arena.alloc(Val::Bool(false)))
}

/// ReflectionFunctionAbstract::hasTentativeReturnType(): bool
pub fn reflection_function_abstract_has_tentative_return_type(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // Stub: Tentative return type tracking not implemented (PHP 8.1+)
    Ok(vm.arena.alloc(Val::Bool(false)))
}

/// ReflectionFunctionAbstract::getTentativeReturnType(): ?ReflectionType
pub fn reflection_function_abstract_get_tentative_return_type(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // Stub: Tentative return type not implemented (PHP 8.1+)
    Ok(vm.arena.alloc(Val::Null))
}

//=============================================================================
// ReflectionFunction Implementation
//=============================================================================

/// ReflectionFunction::__construct(string $name)
pub fn reflection_function_construct(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("ReflectionFunction::__construct() expects exactly 1 argument, 0 given".to_string());
    }

    let this_handle = vm
        .frames
        .last()
        .and_then(|f| f.this)
        .ok_or("ReflectionFunction::__construct() called outside object context")?;

    let func_name_val = vm.arena.get(args[0]).value.clone();
    let func_name_bytes = match func_name_val {
        Val::String(ref s) => s.as_ref(),
        _ => return Err("ReflectionFunction::__construct() expects parameter 1 to be string".to_string()),
    };
    
    let func_sym = vm.context.interner.intern(func_name_bytes);
    
    // Check if function exists (user-defined or native)
    let exists = vm.context.user_functions.contains_key(&func_sym) ||
                 vm.context.engine.registry.get_function(func_name_bytes).is_some();
    
    if !exists {
        let func_name_str = String::from_utf8_lossy(func_name_bytes);
        return Err(format!("Function {}() does not exist", func_name_str));
    }

    // Store function name in object
    let this_obj_handle = if let Val::Object(h) = vm.arena.get(this_handle).value {
        h
    } else {
        return Err("Invalid ReflectionFunction object".to_string());
    };
    
    let name_sym = vm.context.interner.intern(b"name");
    let name_handle = vm.arena.alloc(Val::String(Rc::new(func_name_bytes.to_vec())));
    
    if let Val::ObjPayload(obj_data) = &mut vm.arena.get_mut(this_obj_handle).value {
        obj_data.properties.insert(name_sym, name_handle);
    }

    Ok(vm.arena.alloc(Val::Null))
}

/// ReflectionFunction::getName(): string
pub fn reflection_function_get_name(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let this_handle = vm
        .frames
        .last()
        .and_then(|f| f.this)
        .ok_or("ReflectionFunction::getName() called outside object context")?;

    let name_sym = vm.context.interner.intern(b"name");
    
    let this_obj_handle = if let Val::Object(h) = vm.arena.get(this_handle).value {
        h
    } else {
        return Err("Invalid ReflectionFunction object".to_string());
    };
    
    if let Val::ObjPayload(obj_data) = &vm.arena.get(this_obj_handle).value {
        if let Some(&name_handle) = obj_data.properties.get(&name_sym) {
            return Ok(name_handle);
        }
    }

    Err("ReflectionFunction::getName() failed to retrieve function name".to_string())
}

/// Helper to get ReflectionFunction internal data
fn get_reflection_function_name(vm: &mut VM) -> Result<Symbol, String> {
    let this_handle = vm
        .frames
        .last()
        .and_then(|f| f.this)
        .ok_or("ReflectionFunction method called outside object context")?;

    let name_sym = vm.context.interner.intern(b"name");
    
    let this_obj_handle = if let Val::Object(h) = vm.arena.get(this_handle).value {
        h
    } else {
        return Err("Invalid ReflectionFunction object".to_string());
    };
    
    if let Val::ObjPayload(obj_data) = &vm.arena.get(this_obj_handle).value {
        if let Some(&name_handle) = obj_data.properties.get(&name_sym) {
            let name_val = vm.arena.get(name_handle).value.clone();
            if let Val::String(s) = name_val {
                return Ok(vm.context.interner.intern(s.as_ref()));
            }
        }
    }

    Err("Failed to retrieve ReflectionFunction name".to_string())
}

/// ReflectionFunction::getNumberOfParameters(): int
pub fn reflection_function_get_number_of_parameters(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let func_sym = get_reflection_function_name(vm)?;
    
    if let Some(user_func) = vm.context.user_functions.get(&func_sym) {
        Ok(vm.arena.alloc(Val::Int(user_func.params.len() as i64)))
    } else {
        // Native function - we don't have parameter info, return 0
        Ok(vm.arena.alloc(Val::Int(0)))
    }
}

/// ReflectionFunction::getNumberOfRequiredParameters(): int
pub fn reflection_function_get_number_of_required_parameters(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let func_sym = get_reflection_function_name(vm)?;
    
    if let Some(user_func) = vm.context.user_functions.get(&func_sym) {
        let required = user_func.params.iter()
            .filter(|p| p.default_value.is_none())
            .count();
        Ok(vm.arena.alloc(Val::Int(required as i64)))
    } else {
        Ok(vm.arena.alloc(Val::Int(0)))
    }
}

/// ReflectionFunction::getParameters(): array
pub fn reflection_function_get_parameters(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let func_sym = get_reflection_function_name(vm)?;
    
    // Get the parameter count first to avoid holding a reference to user_functions
    let param_count = if let Some(user_func) = vm.context.user_functions.get(&func_sym) {
        user_func.params.len()
    } else {
        // Native function - return empty array
        return Ok(vm.arena.alloc(Val::Array(Rc::new(ArrayData::new()))));
    };
    
    let mut arr_data = ArrayData::new();
    let reflection_param_sym = vm.context.interner.intern(b"ReflectionParameter");
    
    for idx in 0..param_count {
        // Create ReflectionParameter object
        let obj_data = crate::core::value::ObjectData {
            class: reflection_param_sym,
            properties: indexmap::IndexMap::new(),
            internal: None,
            dynamic_properties: std::collections::HashSet::new(),
        };
        let obj_payload_handle = vm.arena.alloc(Val::ObjPayload(obj_data));
        let obj_handle = vm.arena.alloc(Val::Object(obj_payload_handle));
        
        // Set up this context and call constructor
        let old_this = vm.frames.last_mut().and_then(|f| f.this);
        if let Some(frame) = vm.frames.last_mut() {
            frame.this = Some(obj_handle);
        }
        
        let func_name_bytes = lookup_symbol(vm, func_sym).to_vec();
        let func_name_handle = vm.arena.alloc(Val::String(Rc::new(func_name_bytes)));
        let idx_handle = vm.arena.alloc(Val::Int(idx as i64));
        
        reflection_parameter_construct(vm, &[func_name_handle, idx_handle])?;
        
        // Restore original this
        if let Some(frame) = vm.frames.last_mut() {
            frame.this = old_this;
        }
        
        arr_data.push(obj_handle);
    }
    
    Ok(vm.arena.alloc(Val::Array(Rc::new(arr_data))))
}

/// ReflectionFunction::isUserDefined(): bool
pub fn reflection_function_is_user_defined(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let func_sym = get_reflection_function_name(vm)?;
    let is_user_defined = vm.context.user_functions.contains_key(&func_sym);
    Ok(vm.arena.alloc(Val::Bool(is_user_defined)))
}

/// ReflectionFunction::isInternal(): bool
pub fn reflection_function_is_internal(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let func_sym = get_reflection_function_name(vm)?;
    let is_internal = !vm.context.user_functions.contains_key(&func_sym);
    Ok(vm.arena.alloc(Val::Bool(is_internal)))
}

/// ReflectionFunction::isVariadic(): bool
pub fn reflection_function_is_variadic(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let func_sym = get_reflection_function_name(vm)?;
    
    if let Some(user_func) = vm.context.user_functions.get(&func_sym) {
        let is_variadic = user_func.params.iter().any(|p| p.is_variadic);
        Ok(vm.arena.alloc(Val::Bool(is_variadic)))
    } else {
        Ok(vm.arena.alloc(Val::Bool(false)))
    }
}

/// ReflectionFunction::returnsReference(): bool
pub fn reflection_function_returns_reference(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let func_sym = get_reflection_function_name(vm)?;
    
    if let Some(user_func) = vm.context.user_functions.get(&func_sym) {
        Ok(vm.arena.alloc(Val::Bool(user_func.chunk.returns_ref)))
    } else {
        Ok(vm.arena.alloc(Val::Bool(false)))
    }
}

/// ReflectionFunction::getNamespaceName(): string
pub fn reflection_function_get_namespace_name(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let func_sym = get_reflection_function_name(vm)?;
    let func_name_bytes = lookup_symbol(vm, func_sym);
    
    // Find last backslash to extract namespace
    if let Some(pos) = func_name_bytes.iter().rposition(|&b| b == b'\\') {
        let namespace = &func_name_bytes[..pos];
        Ok(vm.arena.alloc(Val::String(Rc::new(namespace.to_vec()))))
    } else {
        Ok(vm.arena.alloc(Val::String(Rc::new(Vec::new()))))
    }
}

/// ReflectionFunction::getShortName(): string
pub fn reflection_function_get_short_name(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let func_sym = get_reflection_function_name(vm)?;
    let func_name_bytes = lookup_symbol(vm, func_sym);
    
    // Find last backslash to extract short name
    if let Some(pos) = func_name_bytes.iter().rposition(|&b| b == b'\\') {
        let short_name = &func_name_bytes[pos + 1..];
        Ok(vm.arena.alloc(Val::String(Rc::new(short_name.to_vec()))))
    } else {
        Ok(vm.arena.alloc(Val::String(Rc::new(func_name_bytes.to_vec()))))
    }
}

/// ReflectionFunction::inNamespace(): bool
pub fn reflection_function_in_namespace(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let func_sym = get_reflection_function_name(vm)?;
    let func_name_bytes = lookup_symbol(vm, func_sym);
    
    let has_namespace = func_name_bytes.iter().any(|&b| b == b'\\');
    Ok(vm.arena.alloc(Val::Bool(has_namespace)))
}

/// ReflectionFunction::isClosure(): bool
pub fn reflection_function_is_closure(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // For now, all functions are not closures (closures would need special handling)
    Ok(vm.arena.alloc(Val::Bool(false)))
}

/// ReflectionFunction::isGenerator(): bool
pub fn reflection_function_is_generator(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let func_sym = get_reflection_function_name(vm)?;
    
    if let Some(user_func) = vm.context.user_functions.get(&func_sym) {
        Ok(vm.arena.alloc(Val::Bool(user_func.is_generator)))
    } else {
        Ok(vm.arena.alloc(Val::Bool(false)))
    }
}

/// ReflectionFunction::invoke(...$args): mixed
/// Dynamically invoke the function with the given arguments.
pub fn reflection_function_invoke(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    let func_sym = get_reflection_function_name(vm)?;
    let func_name = lookup_symbol(vm, func_sym).to_vec();
    
    // Create function name handle
    let func_name_handle = vm.arena.alloc(Val::String(Rc::new(func_name)));
    
    // Convert args to SmallVec
    let func_args: smallvec::SmallVec<[Handle; 8]> = args.iter().copied().collect();
    
    // Call using the callable system
    vm.call_callable(func_name_handle, func_args)
        .map_err(|e| format!("Function invocation error: {:?}", e))
}

/// ReflectionFunction::invokeArgs(array $args): mixed
/// Invoke the function with arguments as an array.
pub fn reflection_function_invoke_args(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("ReflectionFunction::invokeArgs() expects exactly 1 argument".to_string());
    }
    
    let func_sym = get_reflection_function_name(vm)?;
    let func_name = lookup_symbol(vm, func_sym).to_vec();
    
    // Extract arguments from array
    let args_val = vm.arena.get(args[0]).value.clone();
    let func_args: smallvec::SmallVec<[Handle; 8]> = match args_val {
        Val::Array(ref arr_data) => {
            // Collect array values in order
            let mut result_args = smallvec::SmallVec::new();
            for i in 0..arr_data.map.len() {
                let key = crate::core::value::ArrayKey::Int(i as i64);
                if let Some(&val_handle) = arr_data.map.get(&key) {
                    result_args.push(val_handle);
                } else {
                    break;
                }
            }
            result_args
        }
        _ => {
            return Err("ReflectionFunction::invokeArgs() expects array argument".to_string());
        }
    };
    
    // Create function name handle
    let func_name_handle = vm.arena.alloc(Val::String(Rc::new(func_name)));
    
    // Call using the callable system
    vm.call_callable(func_name_handle, func_args)
        .map_err(|e| format!("Function invocation error: {:?}", e))
}

/// ReflectionFunction::isAnonymous(): bool
/// Check if the function is an anonymous function (closure).
pub fn reflection_function_is_anonymous(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let func_sym = get_reflection_function_name(vm)?;
    let func_name = lookup_symbol(vm, func_sym);
    
    // Anonymous functions typically have names like "{closure}" or contain "{closure}"
    let is_anon = func_name.starts_with(b"{closure}") || 
                  func_name.windows(b"{closure}".len()).any(|w| w == b"{closure}");
    
    Ok(vm.arena.alloc(Val::Bool(is_anon)))
}

/// ReflectionFunction::isDisabled(): bool
/// Check if the function is disabled. Always returns false in this implementation.
pub fn reflection_function_is_disabled(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // In a full implementation, this would check disable_functions ini setting
    // For now, we always return false
    Ok(vm.arena.alloc(Val::Bool(false)))
}

/// ReflectionFunction::__toString(): string
/// Get a string representation of the function.
pub fn reflection_function_to_string(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let func_sym = get_reflection_function_name(vm)?;
    let func_name = lookup_symbol(vm, func_sym);
    
    let mut result = String::new();
    result.push_str("Function [ <");
    
    // Check if user-defined or internal
    if vm.context.user_functions.contains_key(&func_sym) {
        result.push_str("user");
    } else {
        result.push_str("internal");
    }
    result.push_str("> function ");
    result.push_str(&String::from_utf8_lossy(func_name));
    result.push_str(" ] {\n");
    
    // Add basic info (in a full implementation, would include parameters, return type, etc.)
    result.push_str("  @@ (unknown) (unknown)\n");
    result.push_str("}");
    
    Ok(vm.arena.alloc(Val::String(Rc::new(result.into_bytes()))))
}

/// ReflectionFunction::getClosure(): Closure
/// Get a closure representation of the function.
/// Returns null in this implementation as closure conversion is not yet supported.
pub fn reflection_function_get_closure(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // NOTE: Closure wrapping requires:
    // 1. Create Closure object (Val::Closure variant)
    // 2. Store function symbol/chunk reference in closure
    // 3. Bind to null scope (no $this)
    // 4. Return callable Closure object that can be invoked
    Ok(vm.arena.alloc(Val::Null))
}

/// ReflectionFunction::getFileName(): string|false
/// Get the filename where the function is defined.
/// Returns false for internal functions, null for user functions (file tracking not yet implemented).
pub fn reflection_function_get_file_name(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let func_sym = get_reflection_function_name(vm)?;
    
    // Check if it's an internal function
    if !vm.context.user_functions.contains_key(&func_sym) {
        return Ok(vm.arena.alloc(Val::Bool(false)));
    }
    
    // NOTE: File tracking for functions requires:
    // 1. Add file_name: Option<PathBuf> to function metadata
    // 2. Pass file path through parser when compiling functions
    // 3. Store in user_functions map
    Ok(vm.arena.alloc(Val::Null))
}

//=============================================================================
// ReflectionMethod Implementation
//=============================================================================

/// ReflectionMethod::__construct(string|object $class, string $method)
pub fn reflection_method_construct(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() < 2 {
        return Err("ReflectionMethod::__construct() expects exactly 2 arguments".to_string());
    }

    let this_handle = vm
        .frames
        .last()
        .and_then(|f| f.this)
        .ok_or("ReflectionMethod::__construct() called outside object context")?;

    let arg1_val = vm.arena.get(args[0]).value.clone();
    let arg2_val = vm.arena.get(args[1]).value.clone();
    
    // Get class name
    let class_name_sym = match arg1_val {
        Val::String(ref s) => {
            vm.context.interner.intern(s.as_ref())
        }
        Val::Object(payload_handle) => {
            if let Val::ObjPayload(obj_data) = &vm.arena.get(payload_handle).value {
                obj_data.class
            } else {
                return Err("Invalid object payload".to_string());
            }
        }
        _ => {
            return Err("ReflectionMethod::__construct() expects parameter 1 to be string or object".to_string());
        }
    };

    // Get method name
    let original_method_name_bytes = match arg2_val {
        Val::String(s) => s.as_ref().to_vec(),
        _ => {
            return Err("ReflectionMethod::__construct() expects parameter 2 to be string".to_string());
        }
    };
    // PHP stores method names in lowercase for lookup
    let method_name_bytes: Vec<u8> = original_method_name_bytes.iter().map(|b| b.to_ascii_lowercase()).collect();
    let method_name_sym = vm.context.interner.intern(&method_name_bytes);

    // Verify class exists
    let _class_def = get_class_def(vm, class_name_sym)?;
    
    // Verify method exists
    let class_def = get_class_def(vm, class_name_sym)?;
    if !class_def.methods.contains_key(&method_name_sym) {
        let class_name_str = String::from_utf8_lossy(lookup_symbol(vm, class_name_sym));
        let method_name_str = String::from_utf8_lossy(&method_name_bytes);
        return Err(format!("Method {}::{}() does not exist", class_name_str, method_name_str));
    }

    // Store in object properties
    let this_obj_handle = if let Val::Object(h) = vm.arena.get(this_handle).value {
        h
    } else {
        return Err("Invalid ReflectionMethod object".to_string());
    };
    
    let class_sym = vm.context.interner.intern(b"class");
    let method_sym = vm.context.interner.intern(b"method");
    
    let class_name_bytes = lookup_symbol(vm, class_name_sym).to_vec();
    let class_handle = vm.arena.alloc(Val::String(Rc::new(class_name_bytes)));
    let method_handle = vm.arena.alloc(Val::String(Rc::new(original_method_name_bytes)));
    
    if let Val::ObjPayload(obj_data) = &mut vm.arena.get_mut(this_obj_handle).value {
        obj_data.properties.insert(class_sym, class_handle);
        obj_data.properties.insert(method_sym, method_handle);
    }

    Ok(vm.arena.alloc(Val::Null))
}

/// ReflectionMethod::getName(): string
pub fn reflection_method_get_name(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let this_handle = vm
        .frames
        .last()
        .and_then(|f| f.this)
        .ok_or("Method called outside object context")?;

    let method_sym = vm.context.interner.intern(b"method");
    
    let this_obj_handle = if let Val::Object(h) = vm.arena.get(this_handle).value {
        h
    } else {
        return Err("Invalid ReflectionMethod object".to_string());
    };
    
    // Return the original method name string stored in the property (with original casing)
    if let Val::ObjPayload(obj_data) = &vm.arena.get(this_obj_handle).value {
        if let Some(&name_handle) = obj_data.properties.get(&method_sym) {
            return Ok(name_handle);
        }
    }

    Err("ReflectionMethod::getName() failed to retrieve method name".to_string())
}

/// ReflectionMethod::getDeclaringClass(): ReflectionClass
pub fn reflection_method_get_declaring_class(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let data = get_reflection_method_data(vm)?;
    
    // Create a new ReflectionClass object
    let reflection_class_sym = vm.context.interner.intern(b"ReflectionClass");
    let _class_def = get_class_def(vm, reflection_class_sym)?;
    
    let obj_data = crate::core::value::ObjectData {
        class: reflection_class_sym,
        properties: indexmap::IndexMap::new(),
        internal: None,
        dynamic_properties: std::collections::HashSet::new(),
    };
    let obj_payload_handle = vm.arena.alloc(Val::ObjPayload(obj_data));
    let obj_handle = vm.arena.alloc(Val::Object(obj_payload_handle));
    
    // Call ReflectionClass constructor by temporarily setting this
    let old_this = vm.frames.last_mut().and_then(|f| f.this);
    if let Some(frame) = vm.frames.last_mut() {
        frame.this = Some(obj_handle);
    }
    
    let class_name_bytes = lookup_symbol(vm, data.class_name).to_vec();
    let class_name_handle = vm.arena.alloc(Val::String(Rc::new(class_name_bytes)));
    reflection_class_construct(vm, &[class_name_handle])?;
    
    // Restore original this
    if let Some(frame) = vm.frames.last_mut() {
        frame.this = old_this;
    }
    
    Ok(obj_handle)
}

/// ReflectionMethod::getModifiers(): int
pub fn reflection_method_get_modifiers(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let data = get_reflection_method_data(vm)?;
    let class_def = get_class_def(vm, data.class_name)?;
    let method_entry = get_method(&class_def, data.method_name)?;
    
    let mut modifiers = visibility_to_modifiers(method_entry.visibility);
    if method_entry.is_abstract {
        modifiers |= 64; // IS_ABSTRACT
    }
    // Note: is_final not available in MethodEntry
    if method_entry.is_static {
        modifiers |= 16; // IS_STATIC
    }
    
    Ok(vm.arena.alloc(Val::Int(modifiers)))
}

/// ReflectionMethod::isPublic(): bool
pub fn reflection_method_is_public(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let data = get_reflection_method_data(vm)?;
    let class_def = get_class_def(vm, data.class_name)?;
    let method_entry = get_method(&class_def, data.method_name)?;
    Ok(vm.arena.alloc(Val::Bool(matches!(method_entry.visibility, Visibility::Public))))
}

/// ReflectionMethod::isPrivate(): bool
pub fn reflection_method_is_private(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let data = get_reflection_method_data(vm)?;
    let class_def = get_class_def(vm, data.class_name)?;
    let method_entry = get_method(&class_def, data.method_name)?;
    Ok(vm.arena.alloc(Val::Bool(matches!(method_entry.visibility, Visibility::Private))))
}

/// ReflectionMethod::isProtected(): bool
pub fn reflection_method_is_protected(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let data = get_reflection_method_data(vm)?;
    let class_def = get_class_def(vm, data.class_name)?;
    let method_entry = get_method(&class_def, data.method_name)?;
    Ok(vm.arena.alloc(Val::Bool(matches!(method_entry.visibility, Visibility::Protected))))
}

/// ReflectionMethod::isAbstract(): bool
pub fn reflection_method_is_abstract(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let data = get_reflection_method_data(vm)?;
    let class_def = get_class_def(vm, data.class_name)?;
    let method_entry = get_method(&class_def, data.method_name)?;
    Ok(vm.arena.alloc(Val::Bool(method_entry.is_abstract)))
}

/// ReflectionMethod::isFinal(): bool
pub fn reflection_method_is_final(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let _data = get_reflection_method_data(vm)?;
    // Note: is_final not available in MethodEntry, always return false
    Ok(vm.arena.alloc(Val::Bool(false)))
}

/// ReflectionMethod::isStatic(): bool
pub fn reflection_method_is_static(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let data = get_reflection_method_data(vm)?;
    let class_def = get_class_def(vm, data.class_name)?;
    let method_entry = get_method(&class_def, data.method_name)?;
    Ok(vm.arena.alloc(Val::Bool(method_entry.is_static)))
}

/// ReflectionMethod::isConstructor(): bool
pub fn reflection_method_is_constructor(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let data = get_reflection_method_data(vm)?;
    let method_bytes = lookup_symbol(vm, data.method_name);
    let is_constructor = method_bytes == b"__construct";
    Ok(vm.arena.alloc(Val::Bool(is_constructor)))
}

/// ReflectionMethod::isDestructor(): bool
pub fn reflection_method_is_destructor(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let data = get_reflection_method_data(vm)?;
    let method_bytes = lookup_symbol(vm, data.method_name);
    let is_destructor = method_bytes == b"__destruct";
    Ok(vm.arena.alloc(Val::Bool(is_destructor)))
}

/// ReflectionMethod::__toString(): string
pub fn reflection_method_to_string(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let data = get_reflection_method_data(vm)?;
    let class_def = get_class_def(vm, data.class_name)?;
    let method_entry = get_method(&class_def, data.method_name)?;
    
    let class_name = String::from_utf8_lossy(lookup_symbol(vm, data.class_name));
    let method_name = String::from_utf8_lossy(lookup_symbol(vm, data.method_name));
    
    let visibility = match method_entry.visibility {
        Visibility::Public => "public",
        Visibility::Protected => "protected",
        Visibility::Private => "private",
    };
    
    let mut modifiers = vec![visibility.to_string()];
    if method_entry.is_static {
        modifiers.insert(0, "static".to_string());
    }
    if method_entry.is_abstract {
        modifiers.insert(0, "abstract".to_string());
    }
    // Note: is_final not available in MethodEntry
    
    let result = format!(
        "Method [ <user> {} method {}::{} ] {{\n  @@ (unknown) 0 - 0\n}}",
        modifiers.join(" "),
        class_name,
        method_name
    );
    
    Ok(vm.arena.alloc(Val::String(Rc::new(result.into_bytes()))))
}

/// ReflectionMethod::invoke(object $object, mixed ...$args): mixed
pub fn reflection_method_invoke(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("ReflectionMethod::invoke() expects at least 1 argument (object)".to_string());
    }
    
    let data = get_reflection_method_data(vm)?;
    let object_handle = args[0];
    
    // Verify the object is valid
    let obj_val = vm.arena.get(object_handle).value.clone();
    if !matches!(obj_val, Val::Object(_)) {
        return Err("ReflectionMethod::invoke() expects first parameter to be an object".to_string());
    }
    
    // Get method arguments (everything after the object parameter)
    let method_args: smallvec::SmallVec<[Handle; 8]> = if args.len() > 1 {
        args[1..].iter().copied().collect()
    } else {
        smallvec::SmallVec::new()
    };
    
    // Create callable array: [$object, 'methodName']
    let method_name_bytes = lookup_symbol(vm, data.method_name).to_vec();
    let method_name_handle = vm.arena.alloc(Val::String(Rc::new(method_name_bytes)));
    
    let mut arr_data = ArrayData::new();
    arr_data.push(object_handle);
    arr_data.push(method_name_handle);
    let callable_handle = vm.arena.alloc(Val::Array(Rc::new(arr_data)));
    
    // Call using the callable system
    vm.call_callable(callable_handle, method_args)
        .map_err(|e| format!("Method invocation error: {:?}", e))
}

/// ReflectionMethod::invokeArgs(object $object, array $args): mixed
pub fn reflection_method_invoke_args(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() < 2 {
        return Err("ReflectionMethod::invokeArgs() expects exactly 2 arguments".to_string());
    }
    
    let data = get_reflection_method_data(vm)?;
    let object_handle = args[0];
    let args_array_handle = args[1];
    
    // Verify the object is valid
    let obj_val = vm.arena.get(object_handle).value.clone();
    if !matches!(obj_val, Val::Object(_)) {
        return Err("ReflectionMethod::invokeArgs() expects first parameter to be an object".to_string());
    }
    
    // Extract arguments from array
    let args_val = vm.arena.get(args_array_handle).value.clone();
    let method_args: smallvec::SmallVec<[Handle; 8]> = match args_val {
        Val::Array(ref arr_data) => {
            // Collect array values in order
            let mut result_args = smallvec::SmallVec::new();
            for i in 0..arr_data.map.len() {
                let key = crate::core::value::ArrayKey::Int(i as i64);
                if let Some(&val_handle) = arr_data.map.get(&key) {
                    result_args.push(val_handle);
                } else {
                    break;
                }
            }
            result_args
        }
        _ => {
            return Err("ReflectionMethod::invokeArgs() expects second parameter to be an array".to_string());
        }
    };
    
    // Create callable array: [$object, 'methodName']
    let method_name_bytes = lookup_symbol(vm, data.method_name).to_vec();
    let method_name_handle = vm.arena.alloc(Val::String(Rc::new(method_name_bytes)));
    
    let mut arr_data = ArrayData::new();
    arr_data.push(object_handle);
    arr_data.push(method_name_handle);
    let callable_handle = vm.arena.alloc(Val::Array(Rc::new(arr_data)));
    
    // Call using the callable system
    vm.call_callable(callable_handle, method_args)
        .map_err(|e| format!("Method invocation error: {:?}", e))
}

//=============================================================================
// ReflectionParameter Implementation
//=============================================================================

/// ReflectionParameter::__construct(string|array|object $function, int|string $param)
pub fn reflection_parameter_construct(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() < 2 {
        return Err("ReflectionParameter::__construct() expects exactly 2 arguments".to_string());
    }

    let this_handle = vm
        .frames
        .last()
        .and_then(|f| f.this)
        .ok_or("ReflectionParameter::__construct() called outside object context")?;

    let arg1_val = vm.arena.get(args[0]).value.clone();
    let arg2_val = vm.arena.get(args[1]).value.clone();
    
    // Parse function/method specification
    // Can be: "function_name" or ["ClassName", "methodName"] or object
    let (function_name, class_name, is_method) = match arg1_val {
        Val::String(ref s) => {
            // Function name
            (s.as_ref().to_vec(), None, false)
        }
        Val::Array(ref arr_data) => {
            // [class, method] array
            let class_key = crate::core::value::ArrayKey::Int(0);
            let method_key = crate::core::value::ArrayKey::Int(1);
            
            let class_handle = arr_data.map.get(&class_key)
                .ok_or("Invalid array format for ReflectionParameter")?;
            let method_handle = arr_data.map.get(&method_key)
                .ok_or("Invalid array format for ReflectionParameter")?;
            
            let class_val = vm.arena.get(*class_handle).value.clone();
            let method_val = vm.arena.get(*method_handle).value.clone();
            
            let class_bytes = match class_val {
                Val::String(s) => s.as_ref().to_vec(),
                _ => return Err("Class name must be string".to_string()),
            };
            
            let method_bytes = match method_val {
                Val::String(s) => s.as_ref().to_vec(),
                _ => return Err("Method name must be string".to_string()),
            };
            
            (method_bytes, Some(class_bytes), true)
        }
        _ => {
            return Err("ReflectionParameter::__construct() expects parameter 1 to be string or array".to_string());
        }
    };
    
    // Parse parameter specification (can be int index or string name)
    let param_spec = match arg2_val {
        Val::Int(i) => {
            if i < 0 {
                return Err("Parameter index must be non-negative".to_string());
            }
            (i as usize, None)
        }
        Val::String(ref s) => {
            (0, Some(s.as_ref().to_vec()))
        }
        _ => {
            return Err("ReflectionParameter::__construct() expects parameter 2 to be int or string".to_string());
        }
    };
    
    // Get parameter info - just verify it exists and get the name
    let param_name_sym = if is_method {
        // Method parameter
        let class_bytes = class_name.as_ref().ok_or("Missing class name")?;
        let class_sym = vm.context.interner.intern(class_bytes);
        let class_def = get_class_def(vm, class_sym)?;
        
        let method_lowercase: Vec<u8> = function_name.iter().map(|b| b.to_ascii_lowercase()).collect();
        let method_sym = vm.context.interner.intern(&method_lowercase);
        let method_entry = get_method(&class_def, method_sym)?;
        
        // Find parameter by index or name
        if let Some(name_bytes) = param_spec.1 {
            let param_sym = vm.context.interner.intern(&name_bytes);
            let _param = method_entry.signature.parameters.iter()
                .find(|p| p.name == param_sym)
                .ok_or("Parameter does not exist")?;
            param_sym
        } else {
            let param = method_entry.signature.parameters.get(param_spec.0)
                .ok_or("Parameter index out of range")?;
            param.name
        }
    } else {
        // Function parameter
        let func_sym = vm.context.interner.intern(&function_name);
        let user_func = vm.context.user_functions.get(&func_sym)
            .ok_or("Function does not exist")?;
        
        // Find parameter by index or name
        if let Some(name_bytes) = param_spec.1 {
            let param_sym = vm.context.interner.intern(&name_bytes);
            let _param = user_func.params.iter()
                .find(|p| p.name == param_sym)
                .ok_or("Parameter does not exist")?;
            param_sym
        } else {
            let param = user_func.params.get(param_spec.0)
                .ok_or("Parameter index out of range")?;
            param.name
        }
    };
    
    // Store in object properties
    let this_obj_handle = if let Val::Object(h) = vm.arena.get(this_handle).value {
        h
    } else {
        return Err("Invalid ReflectionParameter object".to_string());
    };
    
    let param_name_bytes = lookup_symbol(vm, param_name_sym).to_vec();
    let param_name_handle = vm.arena.alloc(Val::String(Rc::new(param_name_bytes)));
    
    let function_name_handle = vm.arena.alloc(Val::String(Rc::new(function_name)));
    let class_name_handle = if let Some(cn) = class_name {
        vm.arena.alloc(Val::String(Rc::new(cn)))
    } else {
        vm.arena.alloc(Val::Null)
    };
    
    let is_method_handle = vm.arena.alloc(Val::Bool(is_method));
    
    let name_sym = vm.context.interner.intern(b"name");
    let function_sym = vm.context.interner.intern(b"function");
    let class_sym_prop = vm.context.interner.intern(b"class");
    let is_method_sym = vm.context.interner.intern(b"is_method");
    
    if let Val::ObjPayload(obj_data) = &mut vm.arena.get_mut(this_obj_handle).value {
        obj_data.properties.insert(name_sym, param_name_handle);
        obj_data.properties.insert(function_sym, function_name_handle);
        obj_data.properties.insert(class_sym_prop, class_name_handle);
        obj_data.properties.insert(is_method_sym, is_method_handle);
    }

    Ok(vm.arena.alloc(Val::Null))
}

/// Get ReflectionParameter internal data
fn get_reflection_parameter_info(vm: &mut VM) -> Result<(UnifiedParam, Option<Symbol>, Symbol), String> {
    let this_handle = vm
        .frames
        .last()
        .and_then(|f| f.this)
        .ok_or("Method called outside object context")?;

    let name_sym = vm.context.interner.intern(b"name");
    let function_sym = vm.context.interner.intern(b"function");
    let class_sym_prop = vm.context.interner.intern(b"class");
    let is_method_sym = vm.context.interner.intern(b"is_method");
    
    let this_obj_handle = if let Val::Object(h) = vm.arena.get(this_handle).value {
        h
    } else {
        return Err("Invalid ReflectionParameter object".to_string());
    };
    
    if let Val::ObjPayload(obj_data) = &vm.arena.get(this_obj_handle).value {
        let param_name_sym = if let Some(&h) = obj_data.properties.get(&name_sym) {
            if let Val::String(s) = &vm.arena.get(h).value {
                vm.context.interner.intern(s.as_ref())
            } else {
                return Err("Invalid name property".to_string());
            }
        } else {
            return Err("Missing name property".to_string());
        };

        let function_name = if let Some(&h) = obj_data.properties.get(&function_sym) {
            if let Val::String(s) = &vm.arena.get(h).value {
                s.as_ref().to_vec()
            } else {
                return Err("Invalid function property".to_string());
            }
        } else {
            return Err("Missing function property".to_string());
        };

        let class_name = if let Some(&h) = obj_data.properties.get(&class_sym_prop) {
            match &vm.arena.get(h).value {
                Val::String(s) => Some(vm.context.interner.intern(s.as_ref())),
                Val::Null => None,
                _ => return Err("Invalid class property".to_string()),
            }
        } else {
            None
        };

        let is_method = if let Some(&h) = obj_data.properties.get(&is_method_sym) {
            match vm.arena.get(h).value {
                Val::Bool(b) => b,
                _ => return Err("Invalid is_method property".to_string()),
            }
        } else {
            false
        };

        // Get parameter info
        if is_method && class_name.is_some() {
            let class_sym = class_name.unwrap();
            let class_def = get_class_def(vm, class_sym)?;
            
            let method_lowercase: Vec<u8> = function_name.iter().map(|b| b.to_ascii_lowercase()).collect();
            let method_sym = vm.context.interner.intern(&method_lowercase);
            let method_entry = class_def.methods.get(&method_sym)
                .ok_or("Method not found")?;
            
            let param = method_entry.signature.parameters.iter()
                .find(|p| p.name == param_name_sym)
                .ok_or("Parameter not found")?;
            
            return Ok((UnifiedParam::from_parameter_info(param), Some(class_sym), method_sym));
        } else {
            let func_sym = vm.context.interner.intern(&function_name);
            let user_func = vm.context.user_functions.get(&func_sym)
                .ok_or("Function not found")?;
            
            let param = user_func.params.iter()
                .find(|p| p.name == param_name_sym)
                .ok_or("Parameter not found")?;
            
            return Ok((UnifiedParam::from_func_param(param), None, func_sym));
        }
    }

    Err("Failed to retrieve ReflectionParameter data".to_string())
}

/// ReflectionParameter::getName(): string
pub fn reflection_parameter_get_name(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let this_handle = vm
        .frames
        .last()
        .and_then(|f| f.this)
        .ok_or("Method called outside object context")?;

    let name_sym = vm.context.interner.intern(b"name");
    
    let this_obj_handle = if let Val::Object(h) = vm.arena.get(this_handle).value {
        h
    } else {
        return Err("Invalid ReflectionParameter object".to_string());
    };
    
    if let Val::ObjPayload(obj_data) = &vm.arena.get(this_obj_handle).value {
        if let Some(&name_handle) = obj_data.properties.get(&name_sym) {
            return Ok(name_handle);
        }
    }

    Err("ReflectionParameter::getName() failed to retrieve parameter name".to_string())
}

/// ReflectionParameter::isOptional(): bool
pub fn reflection_parameter_is_optional(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let (param, _, _) = get_reflection_parameter_info(vm)?;
    let is_optional = param.default_value.is_some();
    Ok(vm.arena.alloc(Val::Bool(is_optional)))
}

/// ReflectionParameter::isVariadic(): bool
pub fn reflection_parameter_is_variadic(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let (param, _, _) = get_reflection_parameter_info(vm)?;
    Ok(vm.arena.alloc(Val::Bool(param.is_variadic)))
}

/// ReflectionParameter::isPassedByReference(): bool
pub fn reflection_parameter_is_passed_by_reference(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let (param, _, _) = get_reflection_parameter_info(vm)?;
    Ok(vm.arena.alloc(Val::Bool(param.is_reference)))
}

/// ReflectionParameter::hasType(): bool
pub fn reflection_parameter_has_type(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let (param, _, _) = get_reflection_parameter_info(vm)?;
    Ok(vm.arena.alloc(Val::Bool(param.type_hint.is_some())))
}

/// ReflectionParameter::allowsNull(): bool
pub fn reflection_parameter_allows_null(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let (param, _, _) = get_reflection_parameter_info(vm)?;
    
    // Check if type allows null
    let allows_null = match &param.type_hint {
        None => true, // No type hint means anything including null
        Some(TypeHint::Mixed) => true,
        Some(TypeHint::Null) => true,
        Some(TypeHint::Union(types)) => {
            types.iter().any(|t| matches!(t, TypeHint::Null | TypeHint::Mixed))
        }
        _ => false,
    };
    
    Ok(vm.arena.alloc(Val::Bool(allows_null)))
}

/// ReflectionParameter::getDefaultValue(): mixed
pub fn reflection_parameter_get_default_value(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let (param, _, _) = get_reflection_parameter_info(vm)?;
    
    match &param.default_value {
        Some(val) => {
            // Clone and allocate the default value
            let cloned_val: Val = val.clone();
            Ok(vm.arena.alloc(cloned_val))
        }
        None => Err("Parameter does not have a default value".to_string()),
    }
}

/// ReflectionParameter::isDefaultValueAvailable(): bool
pub fn reflection_parameter_is_default_value_available(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let (param, _, _) = get_reflection_parameter_info(vm)?;
    Ok(vm.arena.alloc(Val::Bool(param.default_value.is_some())))
}

/// ReflectionParameter::getPosition(): int
pub fn reflection_parameter_get_position(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let this_handle = vm
        .frames
        .last()
        .and_then(|f| f.this)
        .ok_or("Method called outside object context")?;

    let name_sym = vm.context.interner.intern(b"name");
    let function_sym = vm.context.interner.intern(b"function");
    let class_sym_prop = vm.context.interner.intern(b"class");
    let is_method_sym = vm.context.interner.intern(b"is_method");
    
    let this_obj_handle = if let Val::Object(h) = vm.arena.get(this_handle).value {
        h
    } else {
        return Err("Invalid ReflectionParameter object".to_string());
    };
    
    if let Val::ObjPayload(obj_data) = &vm.arena.get(this_obj_handle).value {
        let param_name_sym = if let Some(&h) = obj_data.properties.get(&name_sym) {
            if let Val::String(s) = &vm.arena.get(h).value {
                vm.context.interner.intern(s.as_ref())
            } else {
                return Err("Invalid name property".to_string());
            }
        } else {
            return Err("Missing name property".to_string());
        };

        let function_name = if let Some(&h) = obj_data.properties.get(&function_sym) {
            if let Val::String(s) = &vm.arena.get(h).value {
                s.as_ref().to_vec()
            } else {
                return Err("Invalid function property".to_string());
            }
        } else {
            return Err("Missing function property".to_string());
        };

        let is_method = if let Some(&h) = obj_data.properties.get(&is_method_sym) {
            match vm.arena.get(h).value {
                Val::Bool(b) => b,
                _ => false,
            }
        } else {
            false
        };

        // Find parameter position
        if is_method {
            let class_name = if let Some(&h) = obj_data.properties.get(&class_sym_prop) {
                match &vm.arena.get(h).value {
                    Val::String(s) => Some(vm.context.interner.intern(s.as_ref())),
                    Val::Null => None,
                    _ => return Err("Invalid class property".to_string()),
                }
            } else {
                None
            };

            if let Some(class_sym) = class_name {
                let class_def = get_class_def(vm, class_sym)?;
                let method_lowercase: Vec<u8> = function_name.iter().map(|b| b.to_ascii_lowercase()).collect();
                let method_sym = vm.context.interner.intern(&method_lowercase);
                let method_entry = get_method(&class_def, method_sym)?;
                
                for (idx, param) in method_entry.signature.parameters.iter().enumerate() {
                    if param.name == param_name_sym {
                        return Ok(vm.arena.alloc(Val::Int(idx as i64)));
                    }
                }
            }
        } else {
            let func_sym = vm.context.interner.intern(&function_name);
            if let Some(user_func) = vm.context.user_functions.get(&func_sym) {
                for (idx, param) in user_func.params.iter().enumerate() {
                    if param.name == param_name_sym {
                        return Ok(vm.arena.alloc(Val::Int(idx as i64)));
                    }
                }
            }
        }
    }

    Err("Unable to determine parameter position".to_string())
}

/// ReflectionParameter::getDeclaringFunction(): ReflectionFunction
pub fn reflection_parameter_get_declaring_function(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let this_handle = vm
        .frames
        .last()
        .and_then(|f| f.this)
        .ok_or("Method called outside object context")?;

    let function_sym = vm.context.interner.intern(b"function");
    let is_method_sym = vm.context.interner.intern(b"is_method");
    
    let this_obj_handle = if let Val::Object(h) = vm.arena.get(this_handle).value {
        h
    } else {
        return Err("Invalid ReflectionParameter object".to_string());
    };
    
    if let Val::ObjPayload(obj_data) = &vm.arena.get(this_obj_handle).value {
        let function_name = if let Some(&h) = obj_data.properties.get(&function_sym) {
            if let Val::String(s) = &vm.arena.get(h).value {
                s.as_ref().to_vec()
            } else {
                return Err("Invalid function property".to_string());
            }
        } else {
            return Err("Missing function property".to_string());
        };

        let is_method = if let Some(&h) = obj_data.properties.get(&is_method_sym) {
            match vm.arena.get(h).value {
                Val::Bool(b) => b,
                _ => false,
            }
        } else {
            false
        };

        // If it's a method, we still return a ReflectionFunction for the function name part
        // (In PHP, you'd use getDeclaringClass() to get class context)
        if is_method {
            // For methods, extract just the function/method name
            let reflection_function_sym = vm.context.interner.intern(b"ReflectionFunction");
            let obj_data = crate::core::value::ObjectData {
                class: reflection_function_sym,
                properties: indexmap::IndexMap::new(),
                internal: None,
                dynamic_properties: std::collections::HashSet::new(),
            };
            let obj_payload_handle = vm.arena.alloc(Val::ObjPayload(obj_data));
            let obj_handle = vm.arena.alloc(Val::Object(obj_payload_handle));
            
            // Set up context and call constructor
            let old_this = vm.frames.last_mut().and_then(|f| f.this);
            if let Some(frame) = vm.frames.last_mut() {
                frame.this = Some(obj_handle);
            }
            
            let func_name_handle = vm.arena.alloc(Val::String(Rc::new(function_name)));
            reflection_function_construct(vm, &[func_name_handle])?;
            
            if let Some(frame) = vm.frames.last_mut() {
                frame.this = old_this;
            }
            
            return Ok(obj_handle);
        } else {
            // Regular function
            let reflection_function_sym = vm.context.interner.intern(b"ReflectionFunction");
            let obj_data = crate::core::value::ObjectData {
                class: reflection_function_sym,
                properties: indexmap::IndexMap::new(),
                internal: None,
                dynamic_properties: std::collections::HashSet::new(),
            };
            let obj_payload_handle = vm.arena.alloc(Val::ObjPayload(obj_data));
            let obj_handle = vm.arena.alloc(Val::Object(obj_payload_handle));
            
            let old_this = vm.frames.last_mut().and_then(|f| f.this);
            if let Some(frame) = vm.frames.last_mut() {
                frame.this = Some(obj_handle);
            }
            
            let func_name_handle = vm.arena.alloc(Val::String(Rc::new(function_name)));
            reflection_function_construct(vm, &[func_name_handle])?;
            
            if let Some(frame) = vm.frames.last_mut() {
                frame.this = old_this;
            }
            
            return Ok(obj_handle);
        }
    }

    Err("Failed to get declaring function".to_string())
}

/// ReflectionParameter::getDeclaringClass(): ?ReflectionClass
pub fn reflection_parameter_get_declaring_class(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let this_handle = vm
        .frames
        .last()
        .and_then(|f| f.this)
        .ok_or("Method called outside object context")?;

    let class_sym_prop = vm.context.interner.intern(b"class");
    let is_method_sym = vm.context.interner.intern(b"is_method");
    
    let this_obj_handle = if let Val::Object(h) = vm.arena.get(this_handle).value {
        h
    } else {
        return Err("Invalid ReflectionParameter object".to_string());
    };
    
    if let Val::ObjPayload(obj_data) = &vm.arena.get(this_obj_handle).value {
        let is_method = if let Some(&h) = obj_data.properties.get(&is_method_sym) {
            match vm.arena.get(h).value {
                Val::Bool(b) => b,
                _ => false,
            }
        } else {
            false
        };

        if !is_method {
            // Function parameter, not a method parameter - return null
            return Ok(vm.arena.alloc(Val::Null));
        }

        let class_name = if let Some(&h) = obj_data.properties.get(&class_sym_prop) {
            match &vm.arena.get(h).value {
                Val::String(s) => s.as_ref().to_vec(),
                Val::Null => return Ok(vm.arena.alloc(Val::Null)),
                _ => return Err("Invalid class property".to_string()),
            }
        } else {
            return Ok(vm.arena.alloc(Val::Null));
        };

        // Create ReflectionClass object
        let reflection_class_sym = vm.context.interner.intern(b"ReflectionClass");
        let obj_data = crate::core::value::ObjectData {
            class: reflection_class_sym,
            properties: indexmap::IndexMap::new(),
            internal: None,
            dynamic_properties: std::collections::HashSet::new(),
        };
        let obj_payload_handle = vm.arena.alloc(Val::ObjPayload(obj_data));
        let obj_handle = vm.arena.alloc(Val::Object(obj_payload_handle));
        
        let old_this = vm.frames.last_mut().and_then(|f| f.this);
        if let Some(frame) = vm.frames.last_mut() {
            frame.this = Some(obj_handle);
        }
        
        let class_name_handle = vm.arena.alloc(Val::String(Rc::new(class_name)));
        reflection_class_construct(vm, &[class_name_handle])?;
        
        if let Some(frame) = vm.frames.last_mut() {
            frame.this = old_this;
        }
        
        return Ok(obj_handle);
    }

    Err("Failed to get declaring class".to_string())
}

/// ReflectionParameter::getType(): ?ReflectionNamedType
pub fn reflection_parameter_get_type(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let (param, _, _) = get_reflection_parameter_info(vm)?;
    
    match &param.type_hint {
        Some(type_hint) => {
            // Check if type allows null (either explicitly nullable or a union with Null)
            let allows_null = match type_hint {
                TypeHint::Union(types) => types.iter().any(|t| matches!(t, TypeHint::Null)),
                TypeHint::Mixed => true, // mixed allows null
                _ => false,
            };
            
            let (type_name, is_builtin) = match type_hint {
                TypeHint::Class(sym) => {
                    let type_name = lookup_symbol(vm, *sym);
                    (String::from_utf8_lossy(type_name).into_owned(), false)
                }
                TypeHint::Int => ("int".to_string(), true),
                TypeHint::Float => ("float".to_string(), true),
                TypeHint::String => ("string".to_string(), true),
                TypeHint::Bool => ("bool".to_string(), true),
                TypeHint::Array => ("array".to_string(), true),
                TypeHint::Callable => ("callable".to_string(), true),
                TypeHint::Iterable => ("iterable".to_string(), true),
                TypeHint::Object => ("object".to_string(), true),
                TypeHint::Mixed => ("mixed".to_string(), true),
                TypeHint::Void => ("void".to_string(), true),
                TypeHint::Never => ("never".to_string(), true),
                TypeHint::Null => ("null".to_string(), true),
                TypeHint::Union(types) => {
                    // For nullable types (e.g., ?int), extract the non-null type
                    if types.len() == 2 && types.iter().any(|t| matches!(t, TypeHint::Null)) {
                        let non_null_type = types.iter().find(|t| !matches!(t, TypeHint::Null)).unwrap();
                        match non_null_type {
                            TypeHint::Int => ("int".to_string(), true),
                            TypeHint::Float => ("float".to_string(), true),
                            TypeHint::String => ("string".to_string(), true),
                            TypeHint::Bool => ("bool".to_string(), true),
                            TypeHint::Array => ("array".to_string(), true),
                            TypeHint::Callable => ("callable".to_string(), true),
                            TypeHint::Iterable => ("iterable".to_string(), true),
                            TypeHint::Object => ("object".to_string(), true),
                            TypeHint::Class(sym) => {
                                let type_name = lookup_symbol(vm, *sym);
                                (String::from_utf8_lossy(type_name).into_owned(), false)
                            }
                            _ => ("union".to_string(), true),
                        }
                    } else {
                        ("union".to_string(), true)
                    }
                },
                TypeHint::Intersection(_) => ("intersection".to_string(), true), // Simplified
            };
            
            // Create ReflectionNamedType object
            let reflection_named_type_sym = vm.context.interner.intern(b"ReflectionNamedType");
            let obj_data = crate::core::value::ObjectData {
                class: reflection_named_type_sym,
                properties: indexmap::IndexMap::new(),
                internal: None,
                dynamic_properties: std::collections::HashSet::new(),
            };
            let obj_payload_handle = vm.arena.alloc(Val::ObjPayload(obj_data));
            let obj_handle = vm.arena.alloc(Val::Object(obj_payload_handle));
            
            let old_this = vm.frames.last_mut().and_then(|f| f.this);
            if let Some(frame) = vm.frames.last_mut() {
                frame.this = Some(obj_handle);
            }
            
            let type_name_handle = vm.arena.alloc(Val::String(Rc::new(type_name.into_bytes())));
            let allows_null_handle = vm.arena.alloc(Val::Bool(allows_null));
            let is_builtin_handle = vm.arena.alloc(Val::Bool(is_builtin));
            
            reflection_named_type_construct(vm, &[type_name_handle, allows_null_handle, is_builtin_handle])?;
            
            if let Some(frame) = vm.frames.last_mut() {
                frame.this = old_this;
            }
            
            Ok(obj_handle)
        }
        None => Ok(vm.arena.alloc(Val::Null)),
    }
}

/// ReflectionParameter::canBePassedByValue(): bool
pub fn reflection_parameter_can_be_passed_by_value(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let (param, _, _) = get_reflection_parameter_info(vm)?;
    // A parameter can be passed by value if it's not passed by reference
    Ok(vm.arena.alloc(Val::Bool(!param.is_reference)))
}

/// ReflectionParameter::isDefaultValueConstant(): bool
pub fn reflection_parameter_is_default_value_constant(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let (param, _, _) = get_reflection_parameter_info(vm)?;
    
    // Check if the parameter has a default value that is a constant expression
    if param.default_value.is_some() {
        // NOTE: Tracking if default is from constant requires:
        // 1. Add is_constant_default: bool to ParameterInfo
        // 2. Detect MyClass::CONST syntax during parsing
        // 3. Store flag alongside default value
        Ok(vm.arena.alloc(Val::Bool(false)))
    } else {
        Ok(vm.arena.alloc(Val::Bool(false)))
    }
}

/// ReflectionParameter::getDefaultValueConstantName(): ?string
pub fn reflection_parameter_get_default_value_constant_name(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let (param, _, _) = get_reflection_parameter_info(vm)?;
    
    // If the default value is a constant, return its name
    if param.default_value.is_some() {
        // NOTE: Requires storing constant_name: Option<String> in ParameterInfo
        // Would store "MyClass::CONST" or "GLOBAL_CONST" as string
        Ok(vm.arena.alloc(Val::Null))
    } else {
        Err("Parameter does not have a default value or it's not a constant".to_string())
    }
}

/// ReflectionParameter::isPromoted(): bool
pub fn reflection_parameter_is_promoted(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let (param, _, _) = get_reflection_parameter_info(vm)?;
    
    // Check if this is a promoted constructor parameter (PHP 8.0+)
    // A promoted parameter becomes a class property automatically
    // NOTE: Requires:
    // 1. Add is_promoted: bool to ParameterInfo
    // 2. Parse 'public Type $param' in constructor parameters
    // 3. Auto-create property in ClassDef during class compilation
    Ok(vm.arena.alloc(Val::Bool(false)))
}

/// ReflectionParameter::getAttributes(): array
pub fn reflection_parameter_get_attributes(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let (_param, _, _) = get_reflection_parameter_info(vm)?;
    
    // NOTE: Parameter attributes (PHP 8.0+) require:
    // 1. Add attributes: Vec<Attribute> to ParameterInfo
    // 2. Parse #[Attr] before parameters
    // 3. Return array of ReflectionAttribute objects
    let array_handle = vm.arena.alloc(Val::Array(Rc::new(ArrayData::new())));
    Ok(array_handle)
}

/// ReflectionParameter::__toString(): string
pub fn reflection_parameter_to_string(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let (param, class_sym_opt, func_sym) = get_reflection_parameter_info(vm)?;
    
    // Get the parameter position
    let position = if let Some(class_sym) = class_sym_opt {
        let class_def = get_class_def(vm, class_sym)?;
        if let Some(method_entry) = class_def.methods.get(&func_sym) {
            method_entry.signature.parameters.iter()
                .position(|p| p.name == param.name)
                .unwrap_or(0)
        } else {
            0
        }
    } else {
        if let Some(user_func) = vm.context.user_functions.get(&func_sym) {
            user_func.params.iter()
                .position(|p| p.name == param.name)
                .unwrap_or(0)
        } else {
            0
        }
    };
    
    let mut result = String::from("Parameter #");
    result.push_str(&position.to_string());
    result.push_str(" [ ");
    
    // Add optional/required indicator
    if param.default_value.is_some() {
        result.push_str("<optional> ");
    } else if param.is_variadic {
        result.push_str("<optional> ");
    } else {
        result.push_str("<required> ");
    }
    
    // Add type if present
    if let Some(ref type_hint) = param.type_hint {
        let type_str = match type_hint {
            TypeHint::Int => "int".to_string(),
            TypeHint::Float => "float".to_string(),
            TypeHint::String => "string".to_string(),
            TypeHint::Bool => "bool".to_string(),
            TypeHint::Array => "array".to_string(),
            TypeHint::Callable => "callable".to_string(),
            TypeHint::Iterable => "iterable".to_string(),
            TypeHint::Object => "object".to_string(),
            TypeHint::Mixed => "mixed".to_string(),
            TypeHint::Void => "void".to_string(),
            TypeHint::Never => "never".to_string(),
            TypeHint::Null => "null".to_string(),
            TypeHint::Class(sym) => {
                let name = lookup_symbol(vm, *sym);
                String::from_utf8_lossy(name).to_string()
            },
            TypeHint::Union(types) => {
                // For nullable types (e.g., ?int), format appropriately
                if types.len() == 2 && types.iter().any(|t| matches!(t, TypeHint::Null)) {
                    let non_null_type = types.iter().find(|t| !matches!(t, TypeHint::Null)).unwrap();
                    let type_name = match non_null_type {
                        TypeHint::Int => "int",
                        TypeHint::String => "string",
                        TypeHint::Bool => "bool",
                        TypeHint::Float => "float",
                        TypeHint::Array => "array",
                        _ => "mixed",
                    };
                    format!("?{}", type_name)
                } else {
                    "mixed".to_string()
                }
            },
            _ => "mixed".to_string(),
        };
        result.push_str(&type_str);
        result.push(' ');
    }
    
    // Add reference indicator
    if param.is_reference {
        result.push('&');
    }
    
    // Add variadic indicator
    if param.is_variadic {
        result.push_str("...");
    }
    
    // Add parameter name
    result.push('$');
    let param_name = lookup_symbol(vm, param.name);
    result.push_str(&String::from_utf8_lossy(param_name));
    
    // Add default value if present
    if let Some(ref default_val) = param.default_value {
        result.push_str(" = ");
        match default_val {
            Val::Int(i) => result.push_str(&i.to_string()),
            Val::Float(f) => result.push_str(&f.to_string()),
            Val::String(s) => {
                result.push('\'');
                result.push_str(&String::from_utf8_lossy(s));
                result.push('\'');
            },
            Val::Bool(b) => result.push_str(if *b { "true" } else { "false" }),
            Val::Null => result.push_str("NULL"),
            Val::Array(_) => result.push_str("Array"),
            _ => result.push_str("..."),
        }
    }
    
    result.push_str(" ]");
    
    Ok(vm.arena.alloc(Val::String(Rc::new(result.into_bytes()))))
}

//=============================================================================
// ReflectionProperty Implementation
//=============================================================================

/// ReflectionProperty::__construct(string|object $class, string $property)
pub fn reflection_property_construct(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() < 2 {
        return Err("ReflectionProperty::__construct() expects exactly 2 arguments".to_string());
    }

    let this_handle = vm
        .frames
        .last()
        .and_then(|f| f.this)
        .ok_or("ReflectionProperty::__construct() called outside object context")?;

    let arg1_val = vm.arena.get(args[0]).value.clone();
    let arg2_val = vm.arena.get(args[1]).value.clone();

    // Parse class specification (can be class name string or object instance)
    let class_name = match arg1_val {
        Val::String(ref s) => s.as_ref().to_vec(),
        Val::Object(obj_payload_handle) => {
            if let Val::ObjPayload(obj_data) = &vm.arena.get(obj_payload_handle).value {
                lookup_symbol(vm, obj_data.class).to_vec()
            } else {
                return Err("Invalid object".to_string());
            }
        }
        _ => {
            return Err("ReflectionProperty::__construct() expects parameter 1 to be string or object".to_string());
        }
    };

    // Parse property name
    let property_name = match arg2_val {
        Val::String(ref s) => s.as_ref().to_vec(),
        _ => {
            return Err("ReflectionProperty::__construct() expects parameter 2 to be string".to_string());
        }
    };

    // Verify property exists (check instance properties in hierarchy)
    let class_sym = vm.context.interner.intern(&class_name);
    let prop_sym = vm.context.interner.intern(&property_name);
    
    // Check instance properties in the inheritance chain
    let has_instance_prop = vm.lookup_property(class_sym, prop_sym).is_some();
    
    // Check static properties in the immediate class (static props aren't inherited in the same way)
    let class_def = get_class_def(vm, class_sym)?;
    let has_static_prop = class_def.static_properties.contains_key(&prop_sym);
    
    if !has_instance_prop && !has_static_prop {
        return Err(format!("Property {}::{} does not exist", 
            String::from_utf8_lossy(&class_name),
            String::from_utf8_lossy(&property_name)));
    }

    // Store in object properties
    let this_obj_handle = if let Val::Object(h) = vm.arena.get(this_handle).value {
        h
    } else {
        return Err("Invalid ReflectionProperty object".to_string());
    };

    let class_name_handle = vm.arena.alloc(Val::String(Rc::new(class_name)));
    let prop_name_handle = vm.arena.alloc(Val::String(Rc::new(property_name)));

    let name_sym = vm.context.interner.intern(b"name");
    let class_sym_prop = vm.context.interner.intern(b"class");

    if let Val::ObjPayload(obj_data) = &mut vm.arena.get_mut(this_obj_handle).value {
        obj_data.properties.insert(name_sym, prop_name_handle);
        obj_data.properties.insert(class_sym_prop, class_name_handle);
    }

    Ok(vm.arena.alloc(Val::Null))
}

/// Helper to get ReflectionProperty data
struct ReflectionPropertyInfo {
    class_name: Symbol,
    property_name: Symbol,
}

fn get_reflection_property_data(vm: &mut VM) -> Result<ReflectionPropertyInfo, String> {
    let this_handle = vm
        .frames
        .last()
        .and_then(|f| f.this)
        .ok_or("Method called outside object context")?;

    let name_sym = vm.context.interner.intern(b"name");
    let class_sym_prop = vm.context.interner.intern(b"class");

    let this_obj_handle = if let Val::Object(h) = vm.arena.get(this_handle).value {
        h
    } else {
        return Err("Invalid ReflectionProperty object".to_string());
    };

    if let Val::ObjPayload(obj_data) = &vm.arena.get(this_obj_handle).value {
        let class_name = if let Some(&h) = obj_data.properties.get(&class_sym_prop) {
            if let Val::String(s) = &vm.arena.get(h).value {
                vm.context.interner.intern(s.as_ref())
            } else {
                return Err("Invalid class property".to_string());
            }
        } else {
            return Err("Missing class property".to_string());
        };

        let property_name = if let Some(&h) = obj_data.properties.get(&name_sym) {
            if let Val::String(s) = &vm.arena.get(h).value {
                vm.context.interner.intern(s.as_ref())
            } else {
                return Err("Invalid name property".to_string());
            }
        } else {
            return Err("Missing name property".to_string());
        };

        return Ok(ReflectionPropertyInfo {
            class_name,
            property_name,
        });
    }

    Err("Failed to retrieve ReflectionProperty data".to_string())
}

/// ReflectionProperty::getName(): string
pub fn reflection_property_get_name(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let data = get_reflection_property_data(vm)?;
    let name_bytes = lookup_symbol(vm, data.property_name).to_vec();
    Ok(vm.arena.alloc(Val::String(Rc::new(name_bytes))))
}

/// ReflectionProperty::getValue(?object $object = null): mixed
pub fn reflection_property_get_value(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    let data = get_reflection_property_data(vm)?;
    
    if args.is_empty() {
        return Err("ReflectionProperty::getValue() expects at least 1 argument for instance properties".to_string());
    }

    let obj_handle = args[0];
    let obj_val = vm.arena.get(obj_handle).value.clone();
    
    let obj_payload_handle = match obj_val {
        Val::Object(h) => h,
        _ => return Err("ReflectionProperty::getValue() expects parameter 1 to be object".to_string()),
    };

    if let Val::ObjPayload(obj_data) = &vm.arena.get(obj_payload_handle).value {
        if let Some(&prop_handle) = obj_data.properties.get(&data.property_name) {
            return Ok(prop_handle);
        }
    }

    // Property doesn't exist on object, return null
    Ok(vm.arena.alloc(Val::Null))
}

/// ReflectionProperty::setValue(object $object, mixed $value): void
pub fn reflection_property_set_value(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() < 2 {
        return Err("ReflectionProperty::setValue() expects exactly 2 arguments".to_string());
    }

    let data = get_reflection_property_data(vm)?;
    let obj_handle = args[0];
    let value_handle = args[1];

    let obj_val = vm.arena.get(obj_handle).value.clone();
    let obj_payload_handle = match obj_val {
        Val::Object(h) => h,
        _ => return Err("ReflectionProperty::setValue() expects parameter 1 to be object".to_string()),
    };

    if let Val::ObjPayload(obj_data) = &mut vm.arena.get_mut(obj_payload_handle).value {
        obj_data.properties.insert(data.property_name, value_handle);
    }

    Ok(vm.arena.alloc(Val::Null))
}

/// ReflectionProperty::isPublic(): bool
pub fn reflection_property_is_public(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let data = get_reflection_property_data(vm)?;
    let class_def = get_class_def(vm, data.class_name)?;
    
    // Check static property first
    if let Some(static_prop) = class_def.static_properties.get(&data.property_name) {
        return Ok(vm.arena.alloc(Val::Bool(static_prop.visibility == Visibility::Public)));
    }
    
    // Check instance property in hierarchy
    if let Some(prop_info) = vm.lookup_property(data.class_name, data.property_name) {
        return Ok(vm.arena.alloc(Val::Bool(prop_info.visibility == Visibility::Public)));
    }
    
    Err("Property not found".to_string())
}

/// ReflectionProperty::isPrivate(): bool
pub fn reflection_property_is_private(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let data = get_reflection_property_data(vm)?;
    let class_def = get_class_def(vm, data.class_name)?;
    
    // Check static property first
    if let Some(static_prop) = class_def.static_properties.get(&data.property_name) {
        return Ok(vm.arena.alloc(Val::Bool(static_prop.visibility == Visibility::Private)));
    }
    
    // Check instance property in hierarchy
    if let Some(prop_info) = vm.lookup_property(data.class_name, data.property_name) {
        return Ok(vm.arena.alloc(Val::Bool(prop_info.visibility == Visibility::Private)));
    }
    
    Err("Property not found".to_string())
}

/// ReflectionProperty::isProtected(): bool
pub fn reflection_property_is_protected(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let data = get_reflection_property_data(vm)?;
    let class_def = get_class_def(vm, data.class_name)?;
    
    // Check static property first
    if let Some(static_prop) = class_def.static_properties.get(&data.property_name) {
        return Ok(vm.arena.alloc(Val::Bool(static_prop.visibility == Visibility::Protected)));
    }
    
    // Check instance property in hierarchy
    if let Some(prop_info) = vm.lookup_property(data.class_name, data.property_name) {
        return Ok(vm.arena.alloc(Val::Bool(prop_info.visibility == Visibility::Protected)));
    }
    
    Err("Property not found".to_string())
}

/// ReflectionProperty::isStatic(): bool
pub fn reflection_property_is_static(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let data = get_reflection_property_data(vm)?;
    let class_def = get_class_def(vm, data.class_name)?;
    
    // Check if property is in static_properties
    let is_static = class_def.static_properties.contains_key(&data.property_name);
    Ok(vm.arena.alloc(Val::Bool(is_static)))
}

/// ReflectionProperty::isDefault(): bool
pub fn reflection_property_is_default(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let data = get_reflection_property_data(vm)?;
    let class_def = get_class_def(vm, data.class_name)?;
    
    // A property is "default" if it's declared in the class (not dynamic)
    // Check both static properties and instance properties in hierarchy
    let is_static = class_def.static_properties.contains_key(&data.property_name);
    let is_instance = vm.lookup_property(data.class_name, data.property_name).is_some();
    
    Ok(vm.arena.alloc(Val::Bool(is_static || is_instance)))
}

/// ReflectionProperty::getModifiers(): int
pub fn reflection_property_get_modifiers(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let data = get_reflection_property_data(vm)?;
    let class_def = get_class_def(vm, data.class_name)?;
    
    let mut modifiers = 0;
    
    // Check if it's a static property first
    if let Some(static_prop) = class_def.static_properties.get(&data.property_name) {
        modifiers |= match static_prop.visibility {
            Visibility::Public => 1,    // IS_PUBLIC
            Visibility::Protected => 2, // IS_PROTECTED
            Visibility::Private => 4,   // IS_PRIVATE
        };
        modifiers |= 16; // IS_STATIC
    } else if let Some(prop_info) = vm.lookup_property(data.class_name, data.property_name) {
        // Instance property in hierarchy
        modifiers |= match prop_info.visibility {
            Visibility::Public => 1,    // IS_PUBLIC
            Visibility::Protected => 2, // IS_PROTECTED
            Visibility::Private => 4,   // IS_PRIVATE
        };
    } else {
        return Err("Property not found".to_string());
    }
    
    Ok(vm.arena.alloc(Val::Int(modifiers as i64)))
}

/// ReflectionProperty::getDeclaringClass(): ReflectionClass
pub fn reflection_property_get_declaring_class(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let data = get_reflection_property_data(vm)?;
    
    // Create ReflectionClass object
    let reflection_class_sym = vm.context.interner.intern(b"ReflectionClass");
    let obj_data = crate::core::value::ObjectData {
        class: reflection_class_sym,
        properties: indexmap::IndexMap::new(),
        internal: None,
        dynamic_properties: std::collections::HashSet::new(),
    };
    let obj_payload_handle = vm.arena.alloc(Val::ObjPayload(obj_data));
    let obj_handle = vm.arena.alloc(Val::Object(obj_payload_handle));
    
    let old_this = vm.frames.last_mut().and_then(|f| f.this);
    if let Some(frame) = vm.frames.last_mut() {
        frame.this = Some(obj_handle);
    }
    
    let class_name_bytes = lookup_symbol(vm, data.class_name).to_vec();
    let class_name_handle = vm.arena.alloc(Val::String(Rc::new(class_name_bytes)));
    reflection_class_construct(vm, &[class_name_handle])?;
    
    if let Some(frame) = vm.frames.last_mut() {
        frame.this = old_this;
    }
    
    Ok(obj_handle)
}

/// ReflectionProperty::__toString(): string
pub fn reflection_property_to_string(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let data = get_reflection_property_data(vm)?;
    let class_def = get_class_def(vm, data.class_name)?;
    
    let class_name = String::from_utf8_lossy(lookup_symbol(vm, data.class_name));
    let prop_name = String::from_utf8_lossy(lookup_symbol(vm, data.property_name));
    
    // Check both static and instance properties (including hierarchy)
    let (visibility, is_static) = if let Some(static_prop) = class_def.static_properties.get(&data.property_name) {
        (static_prop.visibility, true)
    } else if let Some(prop_info) = vm.lookup_property(data.class_name, data.property_name) {
        (prop_info.visibility, false)
    } else {
        return Err("Property not found".to_string());
    };
    
    let visibility_str = match visibility {
        Visibility::Public => "public",
        Visibility::Protected => "protected",
        Visibility::Private => "private",
    };
    
    let static_str = if is_static { "static " } else { "" };
    
    let result = format!(
        "Property [ {}{} ${}::{} ]",
        static_str,
        visibility_str,
        class_name,
        prop_name
    );
    
    Ok(vm.arena.alloc(Val::String(Rc::new(result.into_bytes()))))
}

/// ReflectionProperty::getAttributes(): array
/// Get attributes applied to the property. Returns empty array (attributes not yet implemented).
pub fn reflection_property_get_attributes(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // NOTE: Property attributes require:
    // 1. Add attributes: Vec<Attribute> to property metadata in ClassDef
    // 2. Parse #[Attr] above property declarations
    // 3. Return array of ReflectionAttribute objects
    Ok(vm.arena.alloc(Val::Array(Rc::new(ArrayData::new()))))
}

/// ReflectionProperty::getDefaultValue(): mixed
/// Get the default value of the property.
pub fn reflection_property_get_default_value(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let data = get_reflection_property_data(vm)?;
    let class_def = get_class_def(vm, data.class_name)?;
    
    // Check static properties first
    if let Some(static_prop) = class_def.static_properties.get(&data.property_name) {
        return Ok(vm.arena.alloc(static_prop.value.clone()));
    }
    
    // NOTE: Instance property defaults require:
    // 1. Add default_values: HashMap<Symbol, Val> to ClassDef
    // 2. Store property defaults during class parsing
    // 3. Distinguish between uninitialized and null default
    Ok(vm.arena.alloc(Val::Null))
}

/// ReflectionProperty::getDocComment(): string|false
/// Get doc comment for the property. Returns false (doc comments not yet tracked).
pub fn reflection_property_get_doc_comment(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // NOTE: Requires doc_comment: Option<String> in property metadata
    Ok(vm.arena.alloc(Val::Bool(false)))
}

/// ReflectionProperty::getType(): ?ReflectionType
/// Get the type of the property.
pub fn reflection_property_get_type(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // NOTE: Property type hints require:
    // 1. Add type_hint: Option<TypeHint> to property metadata
    // 2. Parse type declarations: 'public int $x'
    // 3. Return ReflectionNamedType or ReflectionUnionType object
    Ok(vm.arena.alloc(Val::Null))
}

/// ReflectionProperty::hasDefaultValue(): bool
/// Check if the property has a default value.
pub fn reflection_property_has_default_value(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let data = get_reflection_property_data(vm)?;
    let class_def = get_class_def(vm, data.class_name)?;
    
    // Static properties always have values
    if class_def.static_properties.contains_key(&data.property_name) {
        return Ok(vm.arena.alloc(Val::Bool(true)));
    }
    
    // NOTE: Instance properties need default_values tracking in ClassDef
    Ok(vm.arena.alloc(Val::Bool(false)))
}

/// ReflectionProperty::hasType(): bool
/// Check if the property has a type declaration.
pub fn reflection_property_has_type(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // NOTE: Would check if type_hint field is Some(_)
    Ok(vm.arena.alloc(Val::Bool(false)))
}

/// ReflectionProperty::isPromoted(): bool
/// Check if property is constructor-promoted (PHP 8.0+).
pub fn reflection_property_is_promoted(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // NOTE: Requires is_promoted: bool flag in property metadata
    // Set true when property created from promoted constructor parameter
    Ok(vm.arena.alloc(Val::Bool(false)))
}

/// ReflectionProperty::isReadOnly(): bool
/// Check if property is readonly (PHP 8.1+).
pub fn reflection_property_is_readonly(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // NOTE: Requires is_readonly: bool flag in property metadata
    // Parse 'readonly' modifier in property declarations
    Ok(vm.arena.alloc(Val::Bool(false)))
}

/// ReflectionProperty::isInitialized(object $object): bool
/// Check if property is initialized on the given object.
pub fn reflection_property_is_initialized(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("ReflectionProperty::isInitialized() expects exactly 1 argument".to_string());
    }
    
    let data = get_reflection_property_data(vm)?;
    let object_handle = args[0];
    
    // Get object handle
    let obj_payload_handle = match vm.arena.get(object_handle).value {
        Val::Object(h) => h,
        _ => return Err("ReflectionProperty::isInitialized() expects object argument".to_string()),
    };
    
    // Get object data
    if let Val::ObjPayload(obj_data) = &vm.arena.get(obj_payload_handle).value {
        let is_init = obj_data.properties.contains_key(&data.property_name);
        Ok(vm.arena.alloc(Val::Bool(is_init)))
    } else {
        Err("Invalid object data".to_string())
    }
}

/// ReflectionProperty::setAccessible(bool $accessible): void
/// Make private/protected properties accessible (for getValue/setValue).
pub fn reflection_property_set_accessible(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // NOTE: Would store accessible flag in ReflectionProperty object
    // Our implementation already ignores visibility for reflection access
    Ok(vm.arena.alloc(Val::Null))
}

/// ReflectionProperty::getRawDefaultValue(): mixed
/// Get the default value without calling __get.
pub fn reflection_property_get_raw_default_value(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let data = get_reflection_property_data(vm)?;
    let class_def = get_class_def(vm, data.class_name)?;
    
    // Check for static property with default value
    if let Some(static_prop) = class_def.static_properties.get(&data.property_name) {
        return Ok(vm.arena.alloc(static_prop.value.clone()));
    }
    
    // Instance properties don't have default values tracked
    Ok(vm.arena.alloc(Val::Null))
}

/// ReflectionProperty::hasHooks(): bool
/// Check if property has hooks (PHP 8.4+).
pub fn reflection_property_has_hooks(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // NOTE: Property hooks (get/set) are PHP 8.4+ feature:
    // public string $name { get => ...; set => ...; }
    // Requires hooks: Option<PropertyHooks> in property metadata
    Ok(vm.arena.alloc(Val::Bool(false)))
}

/// ReflectionProperty::getHooks(): array
/// Get property hooks (PHP 8.4+).
pub fn reflection_property_get_hooks(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // NOTE: Would return ['get' => Closure, 'set' => Closure]
    Ok(vm.arena.alloc(Val::Array(Rc::new(ArrayData::new()))))
}

/// ReflectionProperty::getSettableType(): ?ReflectionType
/// Get the settable type (may differ from declared type with asymmetric visibility).
pub fn reflection_property_get_settable_type(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // NOTE: Asymmetric visibility (PHP 8.4): public private(set) int $x
    // Requires separate set_type: Option<TypeHint> in property metadata
    Ok(vm.arena.alloc(Val::Null))
}

/// ReflectionProperty::isFinal(): bool
/// Check if property is final (PHP 8.1+).
pub fn reflection_property_is_final(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // NOTE: Final properties prevent overriding in child classes
    // Requires is_final: bool in property metadata
    Ok(vm.arena.alloc(Val::Bool(false)))
}

/// ReflectionProperty::isLazy(): bool
/// Check if property is lazy (PHP 8.4+).
pub fn reflection_property_is_lazy(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // NOTE: Lazy properties only computed on first access
    // Requires is_lazy: bool flag and initializer closure
    Ok(vm.arena.alloc(Val::Bool(false)))
}

/// ReflectionProperty::isVirtual(): bool
/// Check if property is virtual (PHP 8.4+).
pub fn reflection_property_is_virtual(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // NOTE: Virtual properties have hooks but no backing storage
    // Requires is_virtual: bool flag (property exists only through hooks)
    Ok(vm.arena.alloc(Val::Bool(false)))
}

//=============================================================================
// ReflectionClassConstant Implementation
//=============================================================================

/// Helper to store ReflectionClassConstant internal data
struct ReflectionClassConstantData {
    class_name: Symbol,
    constant_name: Symbol,
}

/// Helper to get ReflectionClassConstant data from object
fn get_reflection_class_constant_data(vm: &mut VM) -> Result<ReflectionClassConstantData, String> {
    let this_handle = vm
        .frames
        .last()
        .and_then(|f| f.this)
        .ok_or("ReflectionClassConstant method called outside object context")?;

    let class_sym = vm.context.interner.intern(b"class");
    let name_sym = vm.context.interner.intern(b"name");

    let this_obj_handle = if let Val::Object(h) = vm.arena.get(this_handle).value {
        h
    } else {
        return Err("Invalid ReflectionClassConstant object".to_string());
    };

    if let Val::ObjPayload(obj_data) = &vm.arena.get(this_obj_handle).value {
        let class_name = if let Some(&h) = obj_data.properties.get(&class_sym) {
            if let Val::String(s) = &vm.arena.get(h).value {
                vm.context.interner.intern(s.as_ref())
            } else {
                return Err("Invalid class name".to_string());
            }
        } else {
            return Err("Missing class property".to_string());
        };

        let constant_name = if let Some(&h) = obj_data.properties.get(&name_sym) {
            if let Val::String(s) = &vm.arena.get(h).value {
                vm.context.interner.intern(s.as_ref())
            } else {
                return Err("Invalid constant name".to_string());
            }
        } else {
            return Err("Missing name property".to_string());
        };

        return Ok(ReflectionClassConstantData {
            class_name,
            constant_name,
        });
    }

    Err("Failed to retrieve ReflectionClassConstant data".to_string())
}

/// ReflectionClassConstant::__construct(object|string $class, string $constant)
pub fn reflection_class_constant_construct(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() < 2 {
        return Err("ReflectionClassConstant::__construct() expects exactly 2 arguments".to_string());
    }

    let this_handle = vm
        .frames
        .last()
        .and_then(|f| f.this)
        .ok_or("ReflectionClassConstant::__construct() called outside object context")?;

    // Get class name (from string or object)
    let class_name_bytes = match vm.arena.get(args[0]).value.clone() {
        Val::String(ref s) => s.as_ref().to_vec(),
        Val::Object(obj_h) => {
            if let Val::ObjPayload(obj_data) = &vm.arena.get(obj_h).value {
                lookup_symbol(vm, obj_data.class).to_vec()
            } else {
                return Err("Invalid object".to_string());
            }
        }
        _ => return Err("ReflectionClassConstant::__construct() expects parameter 1 to be string or object".to_string()),
    };

    let class_sym = vm.context.interner.intern(&class_name_bytes);

    // Get constant name
    let const_name_val = vm.arena.get(args[1]).value.clone();
    let const_name_bytes = match const_name_val {
        Val::String(ref s) => s.as_ref().to_vec(),
        _ => return Err("ReflectionClassConstant::__construct() expects parameter 2 to be string".to_string()),
    };

    let const_sym = vm.context.interner.intern(&const_name_bytes);

    // Verify class exists
    let class_def = get_class_def(vm, class_sym)?;

    // Verify constant exists
    if !class_def.constants.contains_key(&const_sym) {
        let class_name_str = String::from_utf8_lossy(&class_name_bytes);
        let const_name_str = String::from_utf8_lossy(&const_name_bytes);
        return Err(format!("Constant {}::{} does not exist", class_name_str, const_name_str));
    }

    // Store in object properties
    let this_obj_handle = if let Val::Object(h) = vm.arena.get(this_handle).value {
        h
    } else {
        return Err("Invalid ReflectionClassConstant object".to_string());
    };

    let class_prop_sym = vm.context.interner.intern(b"class");
    let name_prop_sym = vm.context.interner.intern(b"name");

    let class_handle = vm.arena.alloc(Val::String(Rc::new(class_name_bytes)));
    let name_handle = vm.arena.alloc(Val::String(Rc::new(const_name_bytes)));

    if let Val::ObjPayload(obj_data) = &mut vm.arena.get_mut(this_obj_handle).value {
        obj_data.properties.insert(class_prop_sym, class_handle);
        obj_data.properties.insert(name_prop_sym, name_handle);
    }

    Ok(vm.arena.alloc(Val::Null))
}

/// ReflectionClassConstant::getName(): string
pub fn reflection_class_constant_get_name(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let data = get_reflection_class_constant_data(vm)?;
    let name_bytes = lookup_symbol(vm, data.constant_name).to_vec();
    Ok(vm.arena.alloc(Val::String(Rc::new(name_bytes))))
}

/// ReflectionClassConstant::getValue(): mixed
pub fn reflection_class_constant_get_value(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let data = get_reflection_class_constant_data(vm)?;
    let class_def = get_class_def(vm, data.class_name)?;

    if let Some((const_val, _visibility)) = class_def.constants.get(&data.constant_name) {
        Ok(vm.arena.alloc(const_val.clone()))
    } else {
        Err("Constant not found".to_string())
    }
}

/// ReflectionClassConstant::isPublic(): bool
pub fn reflection_class_constant_is_public(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let data = get_reflection_class_constant_data(vm)?;
    let class_def = get_class_def(vm, data.class_name)?;

    if let Some((_val, visibility)) = class_def.constants.get(&data.constant_name) {
        Ok(vm.arena.alloc(Val::Bool(matches!(visibility, Visibility::Public))))
    } else {
        Err("Constant not found".to_string())
    }
}

/// ReflectionClassConstant::isPrivate(): bool
pub fn reflection_class_constant_is_private(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let data = get_reflection_class_constant_data(vm)?;
    let class_def = get_class_def(vm, data.class_name)?;

    if let Some((_val, visibility)) = class_def.constants.get(&data.constant_name) {
        Ok(vm.arena.alloc(Val::Bool(matches!(visibility, Visibility::Private))))
    } else {
        Err("Constant not found".to_string())
    }
}

/// ReflectionClassConstant::isProtected(): bool
pub fn reflection_class_constant_is_protected(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let data = get_reflection_class_constant_data(vm)?;
    let class_def = get_class_def(vm, data.class_name)?;

    if let Some((_val, visibility)) = class_def.constants.get(&data.constant_name) {
        Ok(vm.arena.alloc(Val::Bool(matches!(visibility, Visibility::Protected))))
    } else {
        Err("Constant not found".to_string())
    }
}

/// ReflectionClassConstant::getModifiers(): int
pub fn reflection_class_constant_get_modifiers(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let data = get_reflection_class_constant_data(vm)?;
    let class_def = get_class_def(vm, data.class_name)?;

    if let Some((_val, visibility)) = class_def.constants.get(&data.constant_name) {
        let modifiers = match visibility {
            Visibility::Public => 1,    // IS_PUBLIC
            Visibility::Protected => 2, // IS_PROTECTED
            Visibility::Private => 4,   // IS_PRIVATE
        };
        Ok(vm.arena.alloc(Val::Int(modifiers as i64)))
    } else {
        Err("Constant not found".to_string())
    }
}

/// ReflectionClassConstant::getDeclaringClass(): ReflectionClass
pub fn reflection_class_constant_get_declaring_class(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let data = get_reflection_class_constant_data(vm)?;

    // Create ReflectionClass object
    let reflection_class_sym = vm.context.interner.intern(b"ReflectionClass");
    let obj_data = crate::core::value::ObjectData {
        class: reflection_class_sym,
        properties: indexmap::IndexMap::new(),
        internal: None,
        dynamic_properties: std::collections::HashSet::new(),
    };
    let obj_payload_handle = vm.arena.alloc(Val::ObjPayload(obj_data));
    let obj_handle = vm.arena.alloc(Val::Object(obj_payload_handle));

    let old_this = vm.frames.last_mut().and_then(|f| f.this);
    if let Some(frame) = vm.frames.last_mut() {
        frame.this = Some(obj_handle);
    }

    let class_name_bytes = lookup_symbol(vm, data.class_name).to_vec();
    let class_name_handle = vm.arena.alloc(Val::String(Rc::new(class_name_bytes)));

    reflection_class_construct(vm, &[class_name_handle])?;

    if let Some(frame) = vm.frames.last_mut() {
        frame.this = old_this;
    }

    Ok(obj_handle)
}

/// ReflectionClassConstant::__toString(): string
pub fn reflection_class_constant_to_string(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let data = get_reflection_class_constant_data(vm)?;
    let class_def = get_class_def(vm, data.class_name)?;

    let class_name = String::from_utf8_lossy(lookup_symbol(vm, data.class_name));
    let const_name = String::from_utf8_lossy(lookup_symbol(vm, data.constant_name));

    if let Some((_val, visibility)) = class_def.constants.get(&data.constant_name) {
        let visibility_str = match visibility {
            Visibility::Public => "public",
            Visibility::Protected => "protected",
            Visibility::Private => "private",
        };

        let result = format!(
            "Constant [ {} {} {}::{} ]",
            visibility_str,
            const_name,
            class_name,
            const_name
        );

        Ok(vm.arena.alloc(Val::String(Rc::new(result.into_bytes()))))
    } else {
        Err("Constant not found".to_string())
    }
}

/// ReflectionClassConstant::getAttributes(): array
/// Get attributes applied to the constant. Returns empty array (attributes not yet implemented).
pub fn reflection_class_constant_get_attributes(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // NOTE: Constant attributes require:
    // 1. Add attributes: Vec<Attribute> to constant metadata in ClassDef
    // 2. Parse #[Attr] above constant declarations
    // 3. Return array of ReflectionAttribute objects
    Ok(vm.arena.alloc(Val::Array(Rc::new(ArrayData::new()))))
}

/// ReflectionClassConstant::getDocComment(): string|false
/// Get doc comment for the constant. Returns false (doc comments not yet tracked).
pub fn reflection_class_constant_get_doc_comment(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // NOTE: Requires doc_comment: Option<String> in constant metadata
    Ok(vm.arena.alloc(Val::Bool(false)))
}

/// ReflectionClassConstant::hasType(): bool
/// Check if the constant has a type declaration (PHP 8.3+).
pub fn reflection_class_constant_has_type(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // NOTE: Typed constants (PHP 8.3+): public const int MAX = 100;
    // Requires type_hint: Option<TypeHint> in constant metadata
    Ok(vm.arena.alloc(Val::Bool(false)))
}

/// ReflectionClassConstant::getType(): ?ReflectionType
/// Get the type of a typed constant (PHP 8.3+). Returns null.
pub fn reflection_class_constant_get_type(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // NOTE: Would return ReflectionNamedType if type_hint is Some(_)
    Ok(vm.arena.alloc(Val::Null))
}

/// ReflectionClassConstant::isEnumCase(): bool
/// Check if the constant is an enum case.
pub fn reflection_class_constant_is_enum_case(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let data = get_reflection_class_constant_data(vm)?;
    let class_def = get_class_def(vm, data.class_name)?;
    
    // Check if the declaring class is an enum
    Ok(vm.arena.alloc(Val::Bool(class_def.is_enum)))
}

/// ReflectionClassConstant::isFinal(): bool
/// Check if the constant is final (PHP 8.1+).
pub fn reflection_class_constant_is_final(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // NOTE: Final constants cannot be overridden in child classes
    // Requires is_final: bool in constant metadata
    Ok(vm.arena.alloc(Val::Bool(false)))
}

/// ReflectionClassConstant::isDeprecated(): bool
/// Check if the constant is deprecated.
pub fn reflection_class_constant_is_deprecated(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // NOTE: Deprecation tracking requires:
    // 1. Add is_deprecated: bool or #[Deprecated] attribute
    // 2. Parse deprecation markers
    // 3. Emit notices when deprecated constants are accessed
    Ok(vm.arena.alloc(Val::Bool(false)))
}

//=============================================================================
// ReflectionConstant Implementation
//=============================================================================

/// Helper to get ReflectionConstant data
struct ReflectionConstantData {
    constant_name: Symbol,
}

fn get_reflection_constant_data(vm: &mut VM) -> Result<ReflectionConstantData, String> {
    let this_handle = vm
        .frames
        .last()
        .and_then(|f| f.this)
        .ok_or("Method called outside object context")?;

    let constant_name_sym = vm.context.interner.intern(b"constantName");

    let this_obj_handle = if let Val::Object(h) = vm.arena.get(this_handle).value {
        h
    } else {
        return Err("Invalid ReflectionConstant object".to_string());
    };

    if let Val::ObjPayload(obj_data) = &vm.arena.get(this_obj_handle).value {
        let constant_name = if let Some(&h) = obj_data.properties.get(&constant_name_sym) {
            if let Val::Int(s) = &vm.arena.get(h).value {
                Symbol(*s as u32)
            } else {
                return Err("Invalid constantName property".to_string());
            }
        } else {
            return Err("Missing constantName property".to_string());
        };

        return Ok(ReflectionConstantData { constant_name });
    }

    Err("Failed to retrieve ReflectionConstant data".to_string())
}

/// new ReflectionConstant(string $name)
pub fn reflection_constant_construct(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("ReflectionConstant::__construct() expects a constant name".to_string());
    }

    let constant_name = match &vm.arena.get(args[0]).value {
        Val::String(s) => s.as_ref().to_vec(),
        _ => return Err("Constant name must be a string".to_string()),
    };

    let constant_sym = vm.context.interner.intern(&constant_name);

    // Check if constant exists
    if !vm.context.constants.contains_key(&constant_sym) {
        return Err(format!(
            "Constant '{}' not found",
            String::from_utf8_lossy(&constant_name)
        ));
    }

    let this_handle = vm
        .frames
        .last()
        .and_then(|f| f.this)
        .ok_or("Constructor called outside object context")?;

    let this_obj_handle = if let Val::Object(h) = vm.arena.get(this_handle).value {
        h
    } else {
        return Err("Invalid object handle".to_string());
    };

    let constant_name_sym_key = vm.context.interner.intern(b"constantName");
    let symbol_handle = vm.arena.alloc(Val::Int(constant_sym.0 as i64));

    if let Val::ObjPayload(obj_data) = &mut vm.arena.get_mut(this_obj_handle).value {
        obj_data.properties.insert(constant_name_sym_key, symbol_handle);
    }

    Ok(vm.arena.alloc(Val::Null))
}

/// ReflectionConstant::getName(): string
pub fn reflection_constant_get_name(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let data = get_reflection_constant_data(vm)?;
    let name = vm.context.interner.lookup(data.constant_name)
        .ok_or("Symbol not found")?;
    Ok(vm.arena.alloc(Val::String(Rc::new(name.to_vec()))))
}

/// ReflectionConstant::getValue(): mixed
pub fn reflection_constant_get_value(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let data = get_reflection_constant_data(vm)?;

    if let Some(val) = vm.context.constants.get(&data.constant_name) {
        Ok(vm.arena.alloc(val.clone()))
    } else {
        Err("Constant not found".to_string())
    }
}

/// ReflectionConstant::getNamespaceName(): string
pub fn reflection_constant_get_namespace_name(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let data = get_reflection_constant_data(vm)?;
    let name = vm.context.interner.lookup(data.constant_name)
        .ok_or("Symbol not found")?;
    let name_str = String::from_utf8_lossy(name);

    if let Some(pos) = name_str.rfind('\\') {
        let namespace = &name_str[..pos];
        Ok(vm.arena.alloc(Val::String(Rc::new(namespace.as_bytes().to_vec()))))
    } else {
        Ok(vm.arena.alloc(Val::String(Rc::new(Vec::new()))))
    }
}

/// ReflectionConstant::getShortName(): string
pub fn reflection_constant_get_short_name(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let data = get_reflection_constant_data(vm)?;
    let name = vm.context.interner.lookup(data.constant_name)
        .ok_or("Symbol not found")?;
    let name_str = String::from_utf8_lossy(name);

    if let Some(pos) = name_str.rfind('\\') {
        let short_name = &name_str[pos + 1..];
        Ok(vm.arena.alloc(Val::String(Rc::new(short_name.as_bytes().to_vec()))))
    } else {
        Ok(vm.arena.alloc(Val::String(Rc::new(name.to_vec()))))
    }
}

/// ReflectionConstant::isDeprecated(): bool
pub fn reflection_constant_is_deprecated(_vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // NOTE: Global constant deprecation tracking
    // Same requirements as class constant deprecation
    Ok(_vm.arena.alloc(Val::Bool(false)))
}

/// ReflectionConstant::getExtension(): ?ReflectionExtension
pub fn reflection_constant_get_extension(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let _data = get_reflection_constant_data(vm)?;
    
    // For now, return null as we don't track which extension defined a constant
    // In a full implementation, we would:
    // 1. Check if constant is internal (defined by core or an extension)
    // 2. Return a ReflectionExtension object for that extension
    // 3. Return null for user-defined constants
    Ok(vm.arena.alloc(Val::Null))
}

/// ReflectionConstant::getExtensionName(): ?string
pub fn reflection_constant_get_extension_name(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let _data = get_reflection_constant_data(vm)?;
    
    // For now, return null as we don't track which extension defined a constant
    // In a full implementation, we would return the extension name (e.g., "Core", "standard", etc.)
    Ok(vm.arena.alloc(Val::Null))
}

/// ReflectionConstant::getFileName(): ?string
pub fn reflection_constant_get_file_name(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let _data = get_reflection_constant_data(vm)?;
    
    // For now, return null as we don't track file locations for constants
    // In a full implementation, we would:
    // 1. Track the file where each user-defined constant was defined
    // 2. Return the file path for user constants
    // 3. Return false (or null) for internal constants
    Ok(vm.arena.alloc(Val::Null))
}

/// ReflectionConstant::__toString(): string
pub fn reflection_constant_to_string(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let data = get_reflection_constant_data(vm)?;
    let name = vm.context.interner.lookup(data.constant_name)
        .ok_or("Symbol not found")?;
    let name_str = String::from_utf8_lossy(name);

    if let Some(val) = vm.context.constants.get(&data.constant_name) {
        let value_str = match val {
            Val::Int(i) => i.to_string(),
            Val::Float(f) => f.to_string(),
            Val::String(s) => format!("'{}'", String::from_utf8_lossy(s)),
            Val::Bool(b) => if *b { "true" } else { "false" }.to_string(),
            Val::Null => "NULL".to_string(),
            _ => "...".to_string(),
        };

        let result = format!("Constant [ {} {} ]", name_str, value_str);
        Ok(vm.arena.alloc(Val::String(Rc::new(result.into_bytes()))))
    } else {
        Err("Constant not found".to_string())
    }
}

//=============================================================================
// ReflectionAttribute Implementation
//=============================================================================

/// Helper to get ReflectionAttribute data
struct ReflectionAttributeData {
    name: Symbol,
    arguments: Vec<Val>,
    target: i64,
    is_repeated: bool,
}

fn get_reflection_attribute_data(vm: &mut VM) -> Result<ReflectionAttributeData, String> {
    let this_handle = vm
        .frames
        .last()
        .and_then(|f| f.this)
        .ok_or("Method called outside object context")?;

    let name_sym = vm.context.interner.intern(b"name");
    let arguments_sym = vm.context.interner.intern(b"arguments");
    let target_sym = vm.context.interner.intern(b"target");
    let is_repeated_sym = vm.context.interner.intern(b"isRepeated");

    let this_obj_handle = if let Val::Object(h) = vm.arena.get(this_handle).value {
        h
    } else {
        return Err("Invalid ReflectionAttribute object".to_string());
    };

    if let Val::ObjPayload(obj_data) = &vm.arena.get(this_obj_handle).value {
        let name = if let Some(&h) = obj_data.properties.get(&name_sym) {
            if let Val::String(s) = &vm.arena.get(h).value {
                vm.context.interner.intern(s.as_ref())
            } else {
                return Err("Invalid name property".to_string());
            }
        } else {
            return Err("Missing name property".to_string());
        };

        let arguments = if let Some(&h) = obj_data.properties.get(&arguments_sym) {
            if let Val::Array(arr) = &vm.arena.get(h).value {
                let mut result = Vec::new();
                for (_k, &v_handle) in arr.map.iter() {
                    result.push(vm.arena.get(v_handle).value.clone());
                }
                result
            } else {
                vec![]
            }
        } else {
            vec![]
        };

        let target = if let Some(&h) = obj_data.properties.get(&target_sym) {
            if let Val::Int(i) = &vm.arena.get(h).value {
                *i
            } else {
                0
            }
        } else {
            0
        };

        let is_repeated = if let Some(&h) = obj_data.properties.get(&is_repeated_sym) {
            if let Val::Bool(b) = &vm.arena.get(h).value {
                *b
            } else {
                false
            }
        } else {
            false
        };

        Ok(ReflectionAttributeData {
            name,
            arguments,
            target,
            is_repeated,
        })
    } else {
        Err("Invalid ReflectionAttribute object".to_string())
    }
}

/// ReflectionAttribute::__construct() - private constructor
pub fn reflection_attribute_construct(_vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // Private constructor - attributes are created internally, not directly instantiated
    Err("Cannot directly instantiate ReflectionAttribute".to_string())
}

/// ReflectionAttribute::getName(): string
pub fn reflection_attribute_get_name(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let data = get_reflection_attribute_data(vm)?;
    let name_bytes = vm.context.interner.lookup(data.name)
        .ok_or("Attribute name not found")?;
    Ok(vm.arena.alloc(Val::String(Rc::new(name_bytes.to_vec()))))
}

/// ReflectionAttribute::getArguments(): array
pub fn reflection_attribute_get_arguments(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let data = get_reflection_attribute_data(vm)?;
    
    let mut arr = ArrayData::new();
    for (i, arg) in data.arguments.iter().enumerate() {
        let arg_handle = vm.arena.alloc(arg.clone());
        let key = ArrayKey::Int(i as i64);
        arr.map.insert(key, arg_handle);
    }
    arr.next_free = data.arguments.len() as i64;
    
    Ok(vm.arena.alloc(Val::Array(Rc::new(arr))))
}

/// ReflectionAttribute::getTarget(): int
pub fn reflection_attribute_get_target(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let data = get_reflection_attribute_data(vm)?;
    Ok(vm.arena.alloc(Val::Int(data.target)))
}

/// ReflectionAttribute::isRepeated(): bool
pub fn reflection_attribute_is_repeated(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let data = get_reflection_attribute_data(vm)?;
    Ok(vm.arena.alloc(Val::Bool(data.is_repeated)))
}

/// ReflectionAttribute::newInstance(): object
/// Instantiates the attribute class represented by this ReflectionAttribute
pub fn reflection_attribute_new_instance(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // NOTE: Attribute class instantiation requires:
    // 1. Get attribute class name from ReflectionAttribute data
    // 2. Look up class definition in context.classes
    // 3. Create object instance with create_object_with_properties
    // 4. Call __construct with stored arguments
    // 5. Return the instantiated attribute object
    // Similar to ReflectionClass::newInstanceArgs()
    Ok(vm.arena.alloc(Val::Null))
}

//=============================================================================
// ReflectionType Implementation (Base Class)
//=============================================================================

/// Helper to get ReflectionType data
struct ReflectionTypeData {
    type_name: Vec<u8>,
    allows_null: bool,
    is_builtin: bool,
}

fn get_reflection_type_data(vm: &mut VM) -> Result<ReflectionTypeData, String> {
    let this_handle = vm
        .frames
        .last()
        .and_then(|f| f.this)
        .ok_or("Method called outside object context")?;

    let type_name_sym = vm.context.interner.intern(b"typeName");
    let allows_null_sym = vm.context.interner.intern(b"allowsNull");
    let is_builtin_sym = vm.context.interner.intern(b"isBuiltin");

    let this_obj_handle = if let Val::Object(h) = vm.arena.get(this_handle).value {
        h
    } else {
        return Err("Invalid ReflectionType object".to_string());
    };

    if let Val::ObjPayload(obj_data) = &vm.arena.get(this_obj_handle).value {
        let type_name = if let Some(&h) = obj_data.properties.get(&type_name_sym) {
            if let Val::String(s) = &vm.arena.get(h).value {
                s.as_ref().to_vec()
            } else {
                return Err("Invalid typeName property".to_string());
            }
        } else {
            return Err("Missing typeName property".to_string());
        };

        let allows_null = if let Some(&h) = obj_data.properties.get(&allows_null_sym) {
            if let Val::Bool(b) = &vm.arena.get(h).value {
                *b
            } else {
                false
            }
        } else {
            false
        };

        let is_builtin = if let Some(&h) = obj_data.properties.get(&is_builtin_sym) {
            if let Val::Bool(b) = &vm.arena.get(h).value {
                *b
            } else {
                false
            }
        } else {
            false
        };

        return Ok(ReflectionTypeData {
            type_name,
            allows_null,
            is_builtin,
        });
    }

    Err("Failed to retrieve ReflectionType data".to_string())
}

/// ReflectionType::allowsNull(): bool
pub fn reflection_type_allows_null(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let data = get_reflection_type_data(vm)?;
    Ok(vm.arena.alloc(Val::Bool(data.allows_null)))
}

/// ReflectionType::getName(): string
pub fn reflection_type_get_name(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let data = get_reflection_type_data(vm)?;
    Ok(vm.arena.alloc(Val::String(Rc::new(data.type_name))))
}

/// ReflectionType::__toString(): string
pub fn reflection_type_to_string(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let data = get_reflection_type_data(vm)?;
    
    let mut result = String::from_utf8_lossy(&data.type_name).to_string();
    if data.allows_null {
        result = format!("?{}", result);
    }
    
    Ok(vm.arena.alloc(Val::String(Rc::new(result.into_bytes()))))
}

//=============================================================================
// ReflectionNamedType Implementation
//=============================================================================

/// ReflectionNamedType::__construct(string $name, bool $allowsNull, bool $isBuiltin)
/// Internal constructor - typically created by other Reflection classes
pub fn reflection_named_type_construct(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.len() < 3 {
        return Err("ReflectionNamedType::__construct() expects 3 arguments".to_string());
    }

    let this_handle = vm
        .frames
        .last()
        .and_then(|f| f.this)
        .ok_or("ReflectionNamedType::__construct() called outside object context")?;

    let type_name = match vm.arena.get(args[0]).value.clone() {
        Val::String(ref s) => s.as_ref().to_vec(),
        _ => return Err("Type name must be a string".to_string()),
    };

    let allows_null = match vm.arena.get(args[1]).value {
        Val::Bool(b) => b,
        _ => false,
    };

    let is_builtin = match vm.arena.get(args[2]).value {
        Val::Bool(b) => b,
        _ => false,
    };

    // Store in object properties
    let this_obj_handle = if let Val::Object(h) = vm.arena.get(this_handle).value {
        h
    } else {
        return Err("Invalid ReflectionNamedType object".to_string());
    };

    let type_name_handle = vm.arena.alloc(Val::String(Rc::new(type_name)));
    let allows_null_handle = vm.arena.alloc(Val::Bool(allows_null));
    let is_builtin_handle = vm.arena.alloc(Val::Bool(is_builtin));

    let type_name_sym = vm.context.interner.intern(b"typeName");
    let allows_null_sym = vm.context.interner.intern(b"allowsNull");
    let is_builtin_sym = vm.context.interner.intern(b"isBuiltin");

    if let Val::ObjPayload(obj_data) = &mut vm.arena.get_mut(this_obj_handle).value {
        obj_data.properties.insert(type_name_sym, type_name_handle);
        obj_data.properties.insert(allows_null_sym, allows_null_handle);
        obj_data.properties.insert(is_builtin_sym, is_builtin_handle);
    }

    Ok(vm.arena.alloc(Val::Null))
}

/// ReflectionNamedType::getName(): string
pub fn reflection_named_type_get_name(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let data = get_reflection_type_data(vm)?;
    Ok(vm.arena.alloc(Val::String(Rc::new(data.type_name))))
}

/// ReflectionNamedType::isBuiltin(): bool
pub fn reflection_named_type_is_builtin(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let data = get_reflection_type_data(vm)?;
    Ok(vm.arena.alloc(Val::Bool(data.is_builtin)))
}

//=============================================================================
// ReflectionUnionType Implementation
//=============================================================================

/// ReflectionUnionType::__construct(array $types)
/// Internal constructor - typically created by other Reflection classes
pub fn reflection_union_type_construct(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("ReflectionUnionType::__construct() expects an array of types".to_string());
    }

    let this_handle = vm
        .frames
        .last()
        .and_then(|f| f.this)
        .ok_or("ReflectionUnionType::__construct() called outside object context")?;

    let types_array = match vm.arena.get(args[0]).value.clone() {
        Val::Array(arr) => arr,
        _ => return Err("Types must be an array".to_string()),
    };

    // Store the types array in object properties
    let this_obj_handle = if let Val::Object(h) = vm.arena.get(this_handle).value {
        h
    } else {
        return Err("Invalid ReflectionUnionType object".to_string());
    };

    if let Val::ObjPayload(obj_data) = &mut vm.arena.get_mut(this_obj_handle).value {
        let types_sym = vm.context.interner.intern(b"types");
        obj_data.properties.insert(types_sym, args[0]);
    }

    Ok(vm.arena.alloc(Val::Null))
}

/// ReflectionUnionType::getTypes(): array
pub fn reflection_union_type_get_types(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let this_handle = vm
        .frames
        .last()
        .and_then(|f| f.this)
        .ok_or("ReflectionUnionType::getTypes() called outside object context")?;

    let types_sym = vm.context.interner.intern(b"types");
    
    let this_obj_handle = if let Val::Object(h) = vm.arena.get(this_handle).value {
        h
    } else {
        return Err("Invalid ReflectionUnionType object".to_string());
    };
    
    if let Val::ObjPayload(obj_data) = &vm.arena.get(this_obj_handle).value {
        if let Some(&types_handle) = obj_data.properties.get(&types_sym) {
            return Ok(types_handle);
        }
    }

    Err("Failed to retrieve union types".to_string())
}

/// ReflectionUnionType::allowsNull(): bool
/// Union types allow null if any of their constituent types is null
pub fn reflection_union_type_allows_null(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let this_handle = vm
        .frames
        .last()
        .and_then(|f| f.this)
        .ok_or("ReflectionUnionType::allowsNull() called outside object context")?;

    let types_sym = vm.context.interner.intern(b"types");
    
    let this_obj_handle = if let Val::Object(h) = vm.arena.get(this_handle).value {
        h
    } else {
        return Err("Invalid ReflectionUnionType object".to_string());
    };
    
    if let Val::ObjPayload(obj_data) = &vm.arena.get(this_obj_handle).value {
        if let Some(&types_handle) = obj_data.properties.get(&types_sym) {
            // Check if any type in the union allows null
            if let Val::Array(arr) = &vm.arena.get(types_handle).value {
                // For simplicity, return false for union types
                // In real PHP, this would check each type
                return Ok(vm.arena.alloc(Val::Bool(false)));
            }
        }
    }

    Ok(vm.arena.alloc(Val::Bool(false)))
}

/// ReflectionUnionType::__toString(): string
pub fn reflection_union_type_to_string(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // For union types, return a simplified representation
    Ok(vm.arena.alloc(Val::String(Rc::new(b"union".to_vec()))))
}

//=============================================================================
// ReflectionIntersectionType Implementation
//=============================================================================

/// ReflectionIntersectionType::__construct(array $types)
/// Internal constructor - typically created by other Reflection classes
pub fn reflection_intersection_type_construct(vm: &mut VM, args: &[Handle]) -> Result<Handle, String> {
    if args.is_empty() {
        return Err("ReflectionIntersectionType::__construct() expects an array of types".to_string());
    }

    let this_handle = vm
        .frames
        .last()
        .and_then(|f| f.this)
        .ok_or("ReflectionIntersectionType::__construct() called outside object context")?;

    let types_array = match vm.arena.get(args[0]).value.clone() {
        Val::Array(arr) => arr,
        _ => return Err("Types must be an array".to_string()),
    };

    // Store the types array in object properties
    let this_obj_handle = if let Val::Object(h) = vm.arena.get(this_handle).value {
        h
    } else {
        return Err("Invalid ReflectionIntersectionType object".to_string());
    };

    if let Val::ObjPayload(obj_data) = &mut vm.arena.get_mut(this_obj_handle).value {
        let types_sym = vm.context.interner.intern(b"types");
        obj_data.properties.insert(types_sym, args[0]);
    }

    Ok(vm.arena.alloc(Val::Null))
}

/// ReflectionIntersectionType::getTypes(): array
pub fn reflection_intersection_type_get_types(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    let this_handle = vm
        .frames
        .last()
        .and_then(|f| f.this)
        .ok_or("ReflectionIntersectionType::getTypes() called outside object context")?;

    let types_sym = vm.context.interner.intern(b"types");
    
    let this_obj_handle = if let Val::Object(h) = vm.arena.get(this_handle).value {
        h
    } else {
        return Err("Invalid ReflectionIntersectionType object".to_string());
    };
    
    if let Val::ObjPayload(obj_data) = &vm.arena.get(this_obj_handle).value {
        if let Some(&types_handle) = obj_data.properties.get(&types_sym) {
            return Ok(types_handle);
        }
    }

    Err("Failed to retrieve intersection types".to_string())
}

/// ReflectionIntersectionType::allowsNull(): bool
/// Intersection types typically don't allow null (all types must be satisfied)
pub fn reflection_intersection_type_allows_null(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // Intersection types don't allow null by default
    Ok(vm.arena.alloc(Val::Bool(false)))
}

/// ReflectionIntersectionType::__toString(): string
pub fn reflection_intersection_type_to_string(vm: &mut VM, _args: &[Handle]) -> Result<Handle, String> {
    // For intersection types, return a simplified representation
    Ok(vm.arena.alloc(Val::String(Rc::new(b"intersection".to_vec()))))
}

//=============================================================================
// Helper Functions for Internal Use
//=============================================================================

/// Get the class name Symbol from a ReflectionClass object's $this context
fn get_reflection_class_name(vm: &mut VM) -> Result<Symbol, String> {
    let this_handle = vm
        .frames
        .last()
        .and_then(|f| f.this)
        .ok_or("Reflection method called outside object context")?;

    let name_sym = vm.context.interner.intern(b"name");
    
    let this_obj_handle = if let Val::Object(h) = vm.arena.get(this_handle).value {
        h
    } else {
        return Err("Invalid ReflectionClass object".to_string());
    };
    
    if let Val::ObjPayload(obj_data) = &vm.arena.get(this_obj_handle).value {
        if let Some(&name_handle) = obj_data.properties.get(&name_sym) {
            let name_val = vm.arena.get(name_handle).value.clone();
            if let Val::String(s) = name_val {
                return Ok(vm.context.interner.intern(s.as_ref()));
            }
        }
    }

    Err("Failed to retrieve ReflectionClass name".to_string())
}

//=============================================================================
// Extension Registration
//=============================================================================

use crate::runtime::extension::{Extension, ExtensionInfo, ExtensionResult};
use crate::runtime::registry::{ExtensionRegistry, NativeClassDef, NativeMethodEntry};

pub struct ReflectionExtension;

impl Extension for ReflectionExtension {
    fn info(&self) -> ExtensionInfo {
        ExtensionInfo {
            name: "reflection",
            version: "8.3.0",
            dependencies: &[],
        }
    }

    fn module_init(&self, registry: &mut ExtensionRegistry) -> ExtensionResult {
        // Register ReflectionClass
        let mut reflection_class_methods = HashMap::new();
        
        reflection_class_methods.insert(
            b"__construct".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_construct,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"getName".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_get_name,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"isAbstract".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_is_abstract,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"isFinal".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_is_final,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"isInterface".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_is_interface,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"isTrait".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_is_trait,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"isEnum".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_is_enum,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"isInstantiable".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_is_instantiable,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"hasMethod".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_has_method,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"hasProperty".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_has_property,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"hasConstant".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_has_constant,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"getMethods".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_get_methods,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"getProperties".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_get_properties,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"getConstants".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_get_constants,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"getConstant".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_get_constant,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"getParentClass".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_get_parent_class,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"getInterfaceNames".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_get_interface_names,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"implementsInterface".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_implements_interface,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"getNamespaceName".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_get_namespace_name,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"getShortName".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_get_short_name,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"inNamespace".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_in_namespace,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"__toString".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_to_string,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"getConstructor".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_get_constructor,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"getMethod".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_get_method,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"getProperty".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_get_property,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"getModifiers".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_get_modifiers,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"isInstance".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_is_instance,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"isSubclassOf".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_is_subclass_of,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"newInstance".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_new_instance,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"newInstanceArgs".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_new_instance_args,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"newInstanceWithoutConstructor".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_new_instance_without_constructor,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"isAnonymous".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_is_anonymous,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"isCloneable".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_is_cloneable,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"isInternal".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_is_internal,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"isUserDefined".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_is_user_defined,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"isIterable".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_is_iterable,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"getAttributes".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_get_attributes,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"getDefaultProperties".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_get_default_properties,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"getDocComment".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_get_doc_comment,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"getFileName".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_get_file_name,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"getStartLine".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_get_start_line,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"getEndLine".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_get_end_line,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"getInterfaces".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_get_interfaces,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"getStaticProperties".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_get_static_properties,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"getStaticPropertyValue".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_get_static_property_value,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"setStaticPropertyValue".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_set_static_property_value,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"getTraitNames".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_get_trait_names,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"getTraits".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_get_traits,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"getTraitAliases".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_get_trait_aliases,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"isReadOnly".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_is_readonly,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"getReflectionConstant".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_get_reflection_constant,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"getReflectionConstants".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_get_reflection_constants,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"getExtension".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_get_extension,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"getExtensionName".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_get_extension_name,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"isIterateable".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_is_iterateable,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"getLazyInitializer".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_get_lazy_initializer,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"initializeLazyObject".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_initialize_lazy_object,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"isUninitializedLazyObject".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_is_uninitialized_lazy_object,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"markLazyObjectAsInitialized".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_mark_lazy_object_as_initialized,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"newLazyGhost".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_new_lazy_ghost,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"newLazyProxy".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_new_lazy_proxy,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"resetAsLazyGhost".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_reset_as_lazy_ghost,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_methods.insert(
            b"resetAsLazyProxy".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_reset_as_lazy_proxy,
                visibility: Visibility::Public,
                is_static: false,
            },
        );

        registry.register_class(NativeClassDef {
            name: b"ReflectionClass".to_vec(),
            parent: None,
            is_interface: false,
            is_trait: false,
            interfaces: vec![b"Reflector".to_vec()],
            methods: reflection_class_methods,
            constants: HashMap::new(),
            constructor: Some(reflection_class_construct),
        });

        // Register ReflectionObject (extends ReflectionClass)
        // ReflectionObject inherits all methods from ReflectionClass
        let mut reflection_object_methods = HashMap::new();
        
        reflection_object_methods.insert(
            b"__construct".to_vec(),
            NativeMethodEntry {
                handler: reflection_object_construct,
                visibility: Visibility::Public,
                is_static: false,
            },
        );

        registry.register_class(NativeClassDef {
            name: b"ReflectionObject".to_vec(),
            parent: Some(b"ReflectionClass".to_vec()),
            is_interface: false,
            is_trait: false,
            interfaces: vec![],
            methods: reflection_object_methods,
            constants: HashMap::new(),
            constructor: Some(reflection_object_construct),
        });

        // Register ReflectionEnum (extends ReflectionClass)
        let mut reflection_enum_methods = HashMap::new();
        
        reflection_enum_methods.insert(
            b"__construct".to_vec(),
            NativeMethodEntry {
                handler: reflection_enum_construct,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_enum_methods.insert(
            b"isBacked".to_vec(),
            NativeMethodEntry {
                handler: reflection_enum_is_backed,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_enum_methods.insert(
            b"getBackingType".to_vec(),
            NativeMethodEntry {
                handler: reflection_enum_get_backing_type,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_enum_methods.insert(
            b"hasCase".to_vec(),
            NativeMethodEntry {
                handler: reflection_enum_has_case,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_enum_methods.insert(
            b"getCase".to_vec(),
            NativeMethodEntry {
                handler: reflection_enum_get_case,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_enum_methods.insert(
            b"getCases".to_vec(),
            NativeMethodEntry {
                handler: reflection_enum_get_cases,
                visibility: Visibility::Public,
                is_static: false,
            },
        );

        registry.register_class(NativeClassDef {
            name: b"ReflectionEnum".to_vec(),
            parent: Some(b"ReflectionClass".to_vec()),
            is_interface: false,
            is_trait: false,
            interfaces: vec![],
            methods: reflection_enum_methods,
            constants: HashMap::new(),
            constructor: Some(reflection_enum_construct),
        });

        // Register ReflectionEnumUnitCase (extends ReflectionClassConstant)
        let mut reflection_enum_unit_case_methods = HashMap::new();
        
        reflection_enum_unit_case_methods.insert(
            b"__construct".to_vec(),
            NativeMethodEntry {
                handler: reflection_enum_unit_case_construct,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_enum_unit_case_methods.insert(
            b"getEnum".to_vec(),
            NativeMethodEntry {
                handler: reflection_enum_unit_case_get_enum,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_enum_unit_case_methods.insert(
            b"getValue".to_vec(),
            NativeMethodEntry {
                handler: reflection_enum_unit_case_get_value,
                visibility: Visibility::Public,
                is_static: false,
            },
        );

        registry.register_class(NativeClassDef {
            name: b"ReflectionEnumUnitCase".to_vec(),
            parent: Some(b"ReflectionClassConstant".to_vec()),
            is_interface: false,
            is_trait: false,
            interfaces: vec![],
            methods: reflection_enum_unit_case_methods,
            constants: HashMap::new(),
            constructor: Some(reflection_enum_unit_case_construct),
        });

        // Register ReflectionEnumBackedCase (extends ReflectionEnumUnitCase)
        let mut reflection_enum_backed_case_methods = HashMap::new();
        
        reflection_enum_backed_case_methods.insert(
            b"getBackingValue".to_vec(),
            NativeMethodEntry {
                handler: reflection_enum_backed_case_get_backing_value,
                visibility: Visibility::Public,
                is_static: false,
            },
        );

        registry.register_class(NativeClassDef {
            name: b"ReflectionEnumBackedCase".to_vec(),
            parent: Some(b"ReflectionEnumUnitCase".to_vec()),
            is_interface: false,
            is_trait: false,
            interfaces: vec![],
            methods: reflection_enum_backed_case_methods,
            constants: HashMap::new(),
            constructor: None, // Inherits constructor from parent
        });

        // Register ReflectionExtension
        let mut reflection_extension_methods = HashMap::new();
        
        reflection_extension_methods.insert(
            b"__construct".to_vec(),
            NativeMethodEntry {
                handler: reflection_extension_construct,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_extension_methods.insert(
            b"getName".to_vec(),
            NativeMethodEntry {
                handler: reflection_extension_get_name,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_extension_methods.insert(
            b"getVersion".to_vec(),
            NativeMethodEntry {
                handler: reflection_extension_get_version,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_extension_methods.insert(
            b"getFunctions".to_vec(),
            NativeMethodEntry {
                handler: reflection_extension_get_functions,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_extension_methods.insert(
            b"getConstants".to_vec(),
            NativeMethodEntry {
                handler: reflection_extension_get_constants,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_extension_methods.insert(
            b"getINIEntries".to_vec(),
            NativeMethodEntry {
                handler: reflection_extension_get_ini_entries,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_extension_methods.insert(
            b"getClasses".to_vec(),
            NativeMethodEntry {
                handler: reflection_extension_get_classes,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_extension_methods.insert(
            b"getClassNames".to_vec(),
            NativeMethodEntry {
                handler: reflection_extension_get_class_names,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_extension_methods.insert(
            b"getDependencies".to_vec(),
            NativeMethodEntry {
                handler: reflection_extension_get_dependencies,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_extension_methods.insert(
            b"info".to_vec(),
            NativeMethodEntry {
                handler: reflection_extension_info,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_extension_methods.insert(
            b"isPersistent".to_vec(),
            NativeMethodEntry {
                handler: reflection_extension_is_persistent,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_extension_methods.insert(
            b"isTemporary".to_vec(),
            NativeMethodEntry {
                handler: reflection_extension_is_temporary,
                visibility: Visibility::Public,
                is_static: false,
            },
        );

        registry.register_class(NativeClassDef {
            name: b"ReflectionExtension".to_vec(),
            parent: None,
            is_interface: false,
            is_trait: false,
            interfaces: vec![],
            methods: reflection_extension_methods,
            constants: HashMap::new(),
            constructor: Some(reflection_extension_construct),
        });

        // Register ReflectionZendExtension
        let mut reflection_zend_extension_methods = HashMap::new();
        
        reflection_zend_extension_methods.insert(
            b"__construct".to_vec(),
            NativeMethodEntry {
                handler: reflection_zend_extension_construct,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_zend_extension_methods.insert(
            b"getName".to_vec(),
            NativeMethodEntry {
                handler: reflection_zend_extension_get_name,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_zend_extension_methods.insert(
            b"getVersion".to_vec(),
            NativeMethodEntry {
                handler: reflection_zend_extension_get_version,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_zend_extension_methods.insert(
            b"getAuthor".to_vec(),
            NativeMethodEntry {
                handler: reflection_zend_extension_get_author,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_zend_extension_methods.insert(
            b"getURL".to_vec(),
            NativeMethodEntry {
                handler: reflection_zend_extension_get_url,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_zend_extension_methods.insert(
            b"getCopyright".to_vec(),
            NativeMethodEntry {
                handler: reflection_zend_extension_get_copyright,
                visibility: Visibility::Public,
                is_static: false,
            },
        );

        registry.register_class(NativeClassDef {
            name: b"ReflectionZendExtension".to_vec(),
            parent: None,
            is_interface: false,
            is_trait: false,
            interfaces: vec![],
            methods: reflection_zend_extension_methods,
            constants: HashMap::new(),
            constructor: Some(reflection_zend_extension_construct),
        });

        // Register ReflectionGenerator
        let mut reflection_generator_methods = HashMap::new();
        
        reflection_generator_methods.insert(
            b"__construct".to_vec(),
            NativeMethodEntry {
                handler: reflection_generator_construct,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_generator_methods.insert(
            b"getExecutingFile".to_vec(),
            NativeMethodEntry {
                handler: reflection_generator_get_executing_file,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_generator_methods.insert(
            b"getExecutingLine".to_vec(),
            NativeMethodEntry {
                handler: reflection_generator_get_executing_line,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_generator_methods.insert(
            b"getExecutingGenerator".to_vec(),
            NativeMethodEntry {
                handler: reflection_generator_get_executing_generator,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_generator_methods.insert(
            b"getFunction".to_vec(),
            NativeMethodEntry {
                handler: reflection_generator_get_function,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_generator_methods.insert(
            b"getThis".to_vec(),
            NativeMethodEntry {
                handler: reflection_generator_get_this,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_generator_methods.insert(
            b"getTrace".to_vec(),
            NativeMethodEntry {
                handler: reflection_generator_get_trace,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_generator_methods.insert(
            b"isClosed".to_vec(),
            NativeMethodEntry {
                handler: reflection_generator_is_closed,
                visibility: Visibility::Public,
                is_static: false,
            },
        );

        registry.register_class(NativeClassDef {
            name: b"ReflectionGenerator".to_vec(),
            parent: None,
            is_interface: false,
            is_trait: false,
            interfaces: vec![],
            methods: reflection_generator_methods,
            constants: HashMap::new(),
            constructor: Some(reflection_generator_construct),
        });

        // Register ReflectionFiber
        let mut reflection_fiber_methods = HashMap::new();
        
        reflection_fiber_methods.insert(
            b"__construct".to_vec(),
            NativeMethodEntry {
                handler: reflection_fiber_construct,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_fiber_methods.insert(
            b"getFiber".to_vec(),
            NativeMethodEntry {
                handler: reflection_fiber_get_fiber,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_fiber_methods.insert(
            b"getCallable".to_vec(),
            NativeMethodEntry {
                handler: reflection_fiber_get_callable,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_fiber_methods.insert(
            b"getExecutingFile".to_vec(),
            NativeMethodEntry {
                handler: reflection_fiber_get_executing_file,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_fiber_methods.insert(
            b"getExecutingLine".to_vec(),
            NativeMethodEntry {
                handler: reflection_fiber_get_executing_line,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_fiber_methods.insert(
            b"getTrace".to_vec(),
            NativeMethodEntry {
                handler: reflection_fiber_get_trace,
                visibility: Visibility::Public,
                is_static: false,
            },
        );

        registry.register_class(NativeClassDef {
            name: b"ReflectionFiber".to_vec(),
            parent: None,
            is_interface: false,
            is_trait: false,
            interfaces: vec![],
            methods: reflection_fiber_methods,
            constants: HashMap::new(),
            constructor: Some(reflection_fiber_construct),
        });

        // Register ReflectionFunctionAbstract (abstract class)
        let mut reflection_function_abstract_methods = HashMap::new();
        
        reflection_function_abstract_methods.insert(
            b"getClosureScopeClass".to_vec(),
            NativeMethodEntry {
                handler: reflection_function_abstract_get_closure_scope_class,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_function_abstract_methods.insert(
            b"getClosureThis".to_vec(),
            NativeMethodEntry {
                handler: reflection_function_abstract_get_closure_this,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_function_abstract_methods.insert(
            b"getClosureUsedVariables".to_vec(),
            NativeMethodEntry {
                handler: reflection_function_abstract_get_closure_used_variables,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_function_abstract_methods.insert(
            b"getDocComment".to_vec(),
            NativeMethodEntry {
                handler: reflection_function_abstract_get_doc_comment,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_function_abstract_methods.insert(
            b"getEndLine".to_vec(),
            NativeMethodEntry {
                handler: reflection_function_abstract_get_end_line,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_function_abstract_methods.insert(
            b"getExtension".to_vec(),
            NativeMethodEntry {
                handler: reflection_function_abstract_get_extension,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_function_abstract_methods.insert(
            b"getExtensionName".to_vec(),
            NativeMethodEntry {
                handler: reflection_function_abstract_get_extension_name,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_function_abstract_methods.insert(
            b"getReturnType".to_vec(),
            NativeMethodEntry {
                handler: reflection_function_abstract_get_return_type,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_function_abstract_methods.insert(
            b"getStartLine".to_vec(),
            NativeMethodEntry {
                handler: reflection_function_abstract_get_start_line,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_function_abstract_methods.insert(
            b"getStaticVariables".to_vec(),
            NativeMethodEntry {
                handler: reflection_function_abstract_get_static_variables,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_function_abstract_methods.insert(
            b"hasReturnType".to_vec(),
            NativeMethodEntry {
                handler: reflection_function_abstract_has_return_type,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_function_abstract_methods.insert(
            b"isDeprecated".to_vec(),
            NativeMethodEntry {
                handler: reflection_function_abstract_is_deprecated,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_function_abstract_methods.insert(
            b"hasTentativeReturnType".to_vec(),
            NativeMethodEntry {
                handler: reflection_function_abstract_has_tentative_return_type,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_function_abstract_methods.insert(
            b"getTentativeReturnType".to_vec(),
            NativeMethodEntry {
                handler: reflection_function_abstract_get_tentative_return_type,
                visibility: Visibility::Public,
                is_static: false,
            },
        );

        registry.register_class(NativeClassDef {
            name: b"ReflectionFunctionAbstract".to_vec(),
            parent: None,
            is_interface: false,
            is_trait: false,
            interfaces: vec![],
            methods: reflection_function_abstract_methods,
            constants: HashMap::new(),
            constructor: None,  // Abstract class - cannot be instantiated
        });

        // Register Reflection (static utility class)
        let mut reflection_methods = HashMap::new();
        
        reflection_methods.insert(
            b"export".to_vec(),
            NativeMethodEntry {
                handler: reflection_export,
                visibility: Visibility::Public,
                is_static: true,
            },
        );
        
        reflection_methods.insert(
            b"getModifierNames".to_vec(),
            NativeMethodEntry {
                handler: reflection_get_modifier_names,
                visibility: Visibility::Public,
                is_static: true,
            },
        );

        registry.register_class(NativeClassDef {
            name: b"Reflection".to_vec(),
            parent: None,
            is_interface: false,
            is_trait: false,
            interfaces: vec![],
            methods: reflection_methods,
            constants: HashMap::new(),
            constructor: None,  // No constructor for static class
        });

        // Register ReflectionException (extends Exception)
        registry.register_class(NativeClassDef {
            name: b"ReflectionException".to_vec(),
            parent: Some(b"Exception".to_vec()),
            is_interface: false,
            is_trait: false,
            interfaces: vec![],
            methods: HashMap::new(),  // Inherits all methods from Exception
            constants: HashMap::new(),
            constructor: None,  // Uses Exception's constructor
        });

        // Register Reflector interface
        registry.register_class(NativeClassDef {
            name: b"Reflector".to_vec(),
            parent: None,
            is_interface: true,  // This is an interface
            is_trait: false,
            interfaces: vec![],
            methods: HashMap::new(),  // Interface methods are abstract
            constants: HashMap::new(),
            constructor: None,
        });

        // Register ReflectionReference
        let mut reflection_reference_methods = HashMap::new();
        
        reflection_reference_methods.insert(
            b"fromArrayElement".to_vec(),
            NativeMethodEntry {
                handler: reflection_reference_from_array_element,
                visibility: Visibility::Public,
                is_static: true,  // Static method
            },
        );
        
        reflection_reference_methods.insert(
            b"getId".to_vec(),
            NativeMethodEntry {
                handler: reflection_reference_get_id,
                visibility: Visibility::Public,
                is_static: false,
            },
        );

        registry.register_class(NativeClassDef {
            name: b"ReflectionReference".to_vec(),
            parent: None,
            is_interface: false,
            is_trait: false,
            interfaces: vec![],
            methods: reflection_reference_methods,
            constants: HashMap::new(),
            constructor: None,  // No explicit constructor, uses default
        });

        // Register ReflectionMethod
        let mut reflection_method_methods = HashMap::new();
        
        reflection_method_methods.insert(
            b"__construct".to_vec(),
            NativeMethodEntry {
                handler: reflection_method_construct,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_method_methods.insert(
            b"getName".to_vec(),
            NativeMethodEntry {
                handler: reflection_method_get_name,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_method_methods.insert(
            b"getDeclaringClass".to_vec(),
            NativeMethodEntry {
                handler: reflection_method_get_declaring_class,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_method_methods.insert(
            b"getModifiers".to_vec(),
            NativeMethodEntry {
                handler: reflection_method_get_modifiers,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_method_methods.insert(
            b"isPublic".to_vec(),
            NativeMethodEntry {
                handler: reflection_method_is_public,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_method_methods.insert(
            b"isPrivate".to_vec(),
            NativeMethodEntry {
                handler: reflection_method_is_private,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_method_methods.insert(
            b"isProtected".to_vec(),
            NativeMethodEntry {
                handler: reflection_method_is_protected,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_method_methods.insert(
            b"isAbstract".to_vec(),
            NativeMethodEntry {
                handler: reflection_method_is_abstract,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_method_methods.insert(
            b"isFinal".to_vec(),
            NativeMethodEntry {
                handler: reflection_method_is_final,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_method_methods.insert(
            b"isStatic".to_vec(),
            NativeMethodEntry {
                handler: reflection_method_is_static,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_method_methods.insert(
            b"isConstructor".to_vec(),
            NativeMethodEntry {
                handler: reflection_method_is_constructor,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_method_methods.insert(
            b"isDestructor".to_vec(),
            NativeMethodEntry {
                handler: reflection_method_is_destructor,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_method_methods.insert(
            b"__toString".to_vec(),
            NativeMethodEntry {
                handler: reflection_method_to_string,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_method_methods.insert(
            b"invoke".to_vec(),
            NativeMethodEntry {
                handler: reflection_method_invoke,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_method_methods.insert(
            b"invokeArgs".to_vec(),
            NativeMethodEntry {
                handler: reflection_method_invoke_args,
                visibility: Visibility::Public,
                is_static: false,
            },
        );

        registry.register_class(NativeClassDef {
            name: b"ReflectionMethod".to_vec(),
            parent: None,
            is_interface: false,
            is_trait: false,
            interfaces: vec![],
            methods: reflection_method_methods,
            constants: HashMap::new(),
            constructor: Some(reflection_method_construct),
        });

        // Register ReflectionParameter
        let mut reflection_parameter_methods = HashMap::new();
        
        reflection_parameter_methods.insert(
            b"__construct".to_vec(),
            NativeMethodEntry {
                handler: reflection_parameter_construct,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_parameter_methods.insert(
            b"getName".to_vec(),
            NativeMethodEntry {
                handler: reflection_parameter_get_name,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_parameter_methods.insert(
            b"isOptional".to_vec(),
            NativeMethodEntry {
                handler: reflection_parameter_is_optional,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_parameter_methods.insert(
            b"isVariadic".to_vec(),
            NativeMethodEntry {
                handler: reflection_parameter_is_variadic,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_parameter_methods.insert(
            b"isPassedByReference".to_vec(),
            NativeMethodEntry {
                handler: reflection_parameter_is_passed_by_reference,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_parameter_methods.insert(
            b"hasType".to_vec(),
            NativeMethodEntry {
                handler: reflection_parameter_has_type,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_parameter_methods.insert(
            b"allowsNull".to_vec(),
            NativeMethodEntry {
                handler: reflection_parameter_allows_null,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_parameter_methods.insert(
            b"getDefaultValue".to_vec(),
            NativeMethodEntry {
                handler: reflection_parameter_get_default_value,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_parameter_methods.insert(
            b"isDefaultValueAvailable".to_vec(),
            NativeMethodEntry {
                handler: reflection_parameter_is_default_value_available,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_parameter_methods.insert(
            b"getPosition".to_vec(),
            NativeMethodEntry {
                handler: reflection_parameter_get_position,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_parameter_methods.insert(
            b"getDeclaringFunction".to_vec(),
            NativeMethodEntry {
                handler: reflection_parameter_get_declaring_function,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_parameter_methods.insert(
            b"getDeclaringClass".to_vec(),
            NativeMethodEntry {
                handler: reflection_parameter_get_declaring_class,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_parameter_methods.insert(
            b"getType".to_vec(),
            NativeMethodEntry {
                handler: reflection_parameter_get_type,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_parameter_methods.insert(
            b"canBePassedByValue".to_vec(),
            NativeMethodEntry {
                handler: reflection_parameter_can_be_passed_by_value,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_parameter_methods.insert(
            b"isDefaultValueConstant".to_vec(),
            NativeMethodEntry {
                handler: reflection_parameter_is_default_value_constant,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_parameter_methods.insert(
            b"getDefaultValueConstantName".to_vec(),
            NativeMethodEntry {
                handler: reflection_parameter_get_default_value_constant_name,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_parameter_methods.insert(
            b"isPromoted".to_vec(),
            NativeMethodEntry {
                handler: reflection_parameter_is_promoted,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_parameter_methods.insert(
            b"getAttributes".to_vec(),
            NativeMethodEntry {
                handler: reflection_parameter_get_attributes,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_parameter_methods.insert(
            b"__toString".to_vec(),
            NativeMethodEntry {
                handler: reflection_parameter_to_string,
                visibility: Visibility::Public,
                is_static: false,
            },
        );

        registry.register_class(NativeClassDef {
            name: b"ReflectionParameter".to_vec(),
            parent: None,
            is_interface: false,
            is_trait: false,
            interfaces: vec![],
            methods: reflection_parameter_methods,
            constants: HashMap::new(),
            constructor: Some(reflection_parameter_construct),
        });

        // Register ReflectionFunction
        let mut reflection_function_methods = HashMap::new();
        
        reflection_function_methods.insert(
            b"__construct".to_vec(),
            NativeMethodEntry {
                handler: reflection_function_construct,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_function_methods.insert(
            b"getName".to_vec(),
            NativeMethodEntry {
                handler: reflection_function_get_name,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_function_methods.insert(
            b"getNumberOfParameters".to_vec(),
            NativeMethodEntry {
                handler: reflection_function_get_number_of_parameters,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_function_methods.insert(
            b"getNumberOfRequiredParameters".to_vec(),
            NativeMethodEntry {
                handler: reflection_function_get_number_of_required_parameters,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_function_methods.insert(
            b"getParameters".to_vec(),
            NativeMethodEntry {
                handler: reflection_function_get_parameters,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_function_methods.insert(
            b"isUserDefined".to_vec(),
            NativeMethodEntry {
                handler: reflection_function_is_user_defined,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_function_methods.insert(
            b"isInternal".to_vec(),
            NativeMethodEntry {
                handler: reflection_function_is_internal,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_function_methods.insert(
            b"isVariadic".to_vec(),
            NativeMethodEntry {
                handler: reflection_function_is_variadic,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_function_methods.insert(
            b"returnsReference".to_vec(),
            NativeMethodEntry {
                handler: reflection_function_returns_reference,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_function_methods.insert(
            b"getNamespaceName".to_vec(),
            NativeMethodEntry {
                handler: reflection_function_get_namespace_name,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_function_methods.insert(
            b"getShortName".to_vec(),
            NativeMethodEntry {
                handler: reflection_function_get_short_name,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_function_methods.insert(
            b"inNamespace".to_vec(),
            NativeMethodEntry {
                handler: reflection_function_in_namespace,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_function_methods.insert(
            b"isClosure".to_vec(),
            NativeMethodEntry {
                handler: reflection_function_is_closure,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_function_methods.insert(
            b"isGenerator".to_vec(),
            NativeMethodEntry {
                handler: reflection_function_is_generator,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_function_methods.insert(
            b"invoke".to_vec(),
            NativeMethodEntry {
                handler: reflection_function_invoke,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_function_methods.insert(
            b"invokeArgs".to_vec(),
            NativeMethodEntry {
                handler: reflection_function_invoke_args,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_function_methods.insert(
            b"isAnonymous".to_vec(),
            NativeMethodEntry {
                handler: reflection_function_is_anonymous,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_function_methods.insert(
            b"isDisabled".to_vec(),
            NativeMethodEntry {
                handler: reflection_function_is_disabled,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_function_methods.insert(
            b"__toString".to_vec(),
            NativeMethodEntry {
                handler: reflection_function_to_string,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_function_methods.insert(
            b"getClosure".to_vec(),
            NativeMethodEntry {
                handler: reflection_function_get_closure,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_function_methods.insert(
            b"getFileName".to_vec(),
            NativeMethodEntry {
                handler: reflection_function_get_file_name,
                visibility: Visibility::Public,
                is_static: false,
            },
        );

        registry.register_class(NativeClassDef {
            name: b"ReflectionFunction".to_vec(),
            parent: None,
            is_interface: false,
            is_trait: false,
            interfaces: vec![],
            methods: reflection_function_methods,
            constants: HashMap::new(),
            constructor: Some(reflection_function_construct),
        });

        // Register ReflectionProperty
        let mut reflection_property_methods = HashMap::new();
        
        reflection_property_methods.insert(
            b"__construct".to_vec(),
            NativeMethodEntry {
                handler: reflection_property_construct,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_property_methods.insert(
            b"getName".to_vec(),
            NativeMethodEntry {
                handler: reflection_property_get_name,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_property_methods.insert(
            b"getValue".to_vec(),
            NativeMethodEntry {
                handler: reflection_property_get_value,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_property_methods.insert(
            b"setValue".to_vec(),
            NativeMethodEntry {
                handler: reflection_property_set_value,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_property_methods.insert(
            b"isPublic".to_vec(),
            NativeMethodEntry {
                handler: reflection_property_is_public,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_property_methods.insert(
            b"isPrivate".to_vec(),
            NativeMethodEntry {
                handler: reflection_property_is_private,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_property_methods.insert(
            b"isProtected".to_vec(),
            NativeMethodEntry {
                handler: reflection_property_is_protected,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_property_methods.insert(
            b"isStatic".to_vec(),
            NativeMethodEntry {
                handler: reflection_property_is_static,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_property_methods.insert(
            b"isDefault".to_vec(),
            NativeMethodEntry {
                handler: reflection_property_is_default,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_property_methods.insert(
            b"getModifiers".to_vec(),
            NativeMethodEntry {
                handler: reflection_property_get_modifiers,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_property_methods.insert(
            b"getDeclaringClass".to_vec(),
            NativeMethodEntry {
                handler: reflection_property_get_declaring_class,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_property_methods.insert(
            b"__toString".to_vec(),
            NativeMethodEntry {
                handler: reflection_property_to_string,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_property_methods.insert(
            b"getAttributes".to_vec(),
            NativeMethodEntry {
                handler: reflection_property_get_attributes,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_property_methods.insert(
            b"getDefaultValue".to_vec(),
            NativeMethodEntry {
                handler: reflection_property_get_default_value,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_property_methods.insert(
            b"getDocComment".to_vec(),
            NativeMethodEntry {
                handler: reflection_property_get_doc_comment,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_property_methods.insert(
            b"getType".to_vec(),
            NativeMethodEntry {
                handler: reflection_property_get_type,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_property_methods.insert(
            b"hasDefaultValue".to_vec(),
            NativeMethodEntry {
                handler: reflection_property_has_default_value,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_property_methods.insert(
            b"hasType".to_vec(),
            NativeMethodEntry {
                handler: reflection_property_has_type,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_property_methods.insert(
            b"isPromoted".to_vec(),
            NativeMethodEntry {
                handler: reflection_property_is_promoted,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_property_methods.insert(
            b"isReadOnly".to_vec(),
            NativeMethodEntry {
                handler: reflection_property_is_readonly,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_property_methods.insert(
            b"isInitialized".to_vec(),
            NativeMethodEntry {
                handler: reflection_property_is_initialized,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_property_methods.insert(
            b"setAccessible".to_vec(),
            NativeMethodEntry {
                handler: reflection_property_set_accessible,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_property_methods.insert(
            b"getRawDefaultValue".to_vec(),
            NativeMethodEntry {
                handler: reflection_property_get_raw_default_value,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_property_methods.insert(
            b"hasHooks".to_vec(),
            NativeMethodEntry {
                handler: reflection_property_has_hooks,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_property_methods.insert(
            b"getHooks".to_vec(),
            NativeMethodEntry {
                handler: reflection_property_get_hooks,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_property_methods.insert(
            b"getSettableType".to_vec(),
            NativeMethodEntry {
                handler: reflection_property_get_settable_type,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_property_methods.insert(
            b"isFinal".to_vec(),
            NativeMethodEntry {
                handler: reflection_property_is_final,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_property_methods.insert(
            b"isLazy".to_vec(),
            NativeMethodEntry {
                handler: reflection_property_is_lazy,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_property_methods.insert(
            b"isVirtual".to_vec(),
            NativeMethodEntry {
                handler: reflection_property_is_virtual,
                visibility: Visibility::Public,
                is_static: false,
            },
        );

        registry.register_class(NativeClassDef {
            name: b"ReflectionProperty".to_vec(),
            parent: None,
            is_interface: false,
            is_trait: false,
            interfaces: vec![],
            methods: reflection_property_methods,
            constants: HashMap::new(),
            constructor: Some(reflection_property_construct),
        });

        // Register ReflectionClassConstant
        let mut reflection_class_constant_methods = HashMap::new();
        
        reflection_class_constant_methods.insert(
            b"__construct".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_constant_construct,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_constant_methods.insert(
            b"getName".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_constant_get_name,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_constant_methods.insert(
            b"getValue".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_constant_get_value,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_constant_methods.insert(
            b"isPublic".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_constant_is_public,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_constant_methods.insert(
            b"isPrivate".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_constant_is_private,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_constant_methods.insert(
            b"isProtected".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_constant_is_protected,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_constant_methods.insert(
            b"getModifiers".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_constant_get_modifiers,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_constant_methods.insert(
            b"getDeclaringClass".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_constant_get_declaring_class,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_constant_methods.insert(
            b"__toString".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_constant_to_string,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_constant_methods.insert(
            b"getAttributes".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_constant_get_attributes,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_constant_methods.insert(
            b"getDocComment".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_constant_get_doc_comment,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_constant_methods.insert(
            b"hasType".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_constant_has_type,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_constant_methods.insert(
            b"getType".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_constant_get_type,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_constant_methods.insert(
            b"isEnumCase".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_constant_is_enum_case,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_constant_methods.insert(
            b"isFinal".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_constant_is_final,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_class_constant_methods.insert(
            b"isDeprecated".to_vec(),
            NativeMethodEntry {
                handler: reflection_class_constant_is_deprecated,
                visibility: Visibility::Public,
                is_static: false,
            },
        );

        registry.register_class(NativeClassDef {
            name: b"ReflectionClassConstant".to_vec(),
            parent: None,
            is_interface: false,
            is_trait: false,
            interfaces: vec![],
            methods: reflection_class_constant_methods,
            constants: HashMap::new(),
            constructor: Some(reflection_class_constant_construct),
        });

        // Register ReflectionConstant
        let mut reflection_constant_methods = HashMap::new();
        
        reflection_constant_methods.insert(
            b"__construct".to_vec(),
            NativeMethodEntry {
                handler: reflection_constant_construct,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_constant_methods.insert(
            b"getName".to_vec(),
            NativeMethodEntry {
                handler: reflection_constant_get_name,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_constant_methods.insert(
            b"getValue".to_vec(),
            NativeMethodEntry {
                handler: reflection_constant_get_value,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_constant_methods.insert(
            b"getNamespaceName".to_vec(),
            NativeMethodEntry {
                handler: reflection_constant_get_namespace_name,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_constant_methods.insert(
            b"getShortName".to_vec(),
            NativeMethodEntry {
                handler: reflection_constant_get_short_name,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_constant_methods.insert(
            b"isDeprecated".to_vec(),
            NativeMethodEntry {
                handler: reflection_constant_is_deprecated,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_constant_methods.insert(
            b"getExtension".to_vec(),
            NativeMethodEntry {
                handler: reflection_constant_get_extension,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_constant_methods.insert(
            b"getExtensionName".to_vec(),
            NativeMethodEntry {
                handler: reflection_constant_get_extension_name,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_constant_methods.insert(
            b"getFileName".to_vec(),
            NativeMethodEntry {
                handler: reflection_constant_get_file_name,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_constant_methods.insert(
            b"__toString".to_vec(),
            NativeMethodEntry {
                handler: reflection_constant_to_string,
                visibility: Visibility::Public,
                is_static: false,
            },
        );

        registry.register_class(NativeClassDef {
            name: b"ReflectionConstant".to_vec(),
            parent: None,
            is_interface: false,
            is_trait: false,
            interfaces: vec![],
            methods: reflection_constant_methods,
            constants: HashMap::new(),
            constructor: Some(reflection_constant_construct),
        });

        // Register ReflectionAttribute
        let mut reflection_attribute_methods = HashMap::new();
        
        reflection_attribute_methods.insert(
            b"__construct".to_vec(),
            NativeMethodEntry {
                handler: reflection_attribute_construct,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_attribute_methods.insert(
            b"getName".to_vec(),
            NativeMethodEntry {
                handler: reflection_attribute_get_name,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_attribute_methods.insert(
            b"getArguments".to_vec(),
            NativeMethodEntry {
                handler: reflection_attribute_get_arguments,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_attribute_methods.insert(
            b"getTarget".to_vec(),
            NativeMethodEntry {
                handler: reflection_attribute_get_target,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_attribute_methods.insert(
            b"isRepeated".to_vec(),
            NativeMethodEntry {
                handler: reflection_attribute_is_repeated,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_attribute_methods.insert(
            b"newInstance".to_vec(),
            NativeMethodEntry {
                handler: reflection_attribute_new_instance,
                visibility: Visibility::Public,
                is_static: false,
            },
        );

        let mut reflection_attribute_constants = HashMap::new();
        reflection_attribute_constants.insert(
            b"IS_INSTANCEOF".to_vec(),
            (Val::Int(2), Visibility::Public),
        );

        registry.register_class(NativeClassDef {
            name: b"ReflectionAttribute".to_vec(),
            parent: None,
            is_interface: false,
            is_trait: false,
            interfaces: vec![],
            methods: reflection_attribute_methods,
            constants: reflection_attribute_constants,
            constructor: Some(reflection_attribute_construct),
        });

        // Register ReflectionType (base class)
        let mut reflection_type_methods = HashMap::new();
        
        reflection_type_methods.insert(
            b"allowsNull".to_vec(),
            NativeMethodEntry {
                handler: reflection_type_allows_null,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_type_methods.insert(
            b"getName".to_vec(),
            NativeMethodEntry {
                handler: reflection_type_get_name,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_type_methods.insert(
            b"__toString".to_vec(),
            NativeMethodEntry {
                handler: reflection_type_to_string,
                visibility: Visibility::Public,
                is_static: false,
            },
        );

        registry.register_class(NativeClassDef {
            name: b"ReflectionType".to_vec(),
            parent: None,
            is_interface: false,
            is_trait: false,
            interfaces: vec![],
            methods: reflection_type_methods,
            constants: HashMap::new(),
            constructor: None, // Abstract-like base class
        });

        // Register ReflectionNamedType (extends ReflectionType)
        let mut reflection_named_type_methods = HashMap::new();
        
        reflection_named_type_methods.insert(
            b"__construct".to_vec(),
            NativeMethodEntry {
                handler: reflection_named_type_construct,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_named_type_methods.insert(
            b"getName".to_vec(),
            NativeMethodEntry {
                handler: reflection_named_type_get_name,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_named_type_methods.insert(
            b"isBuiltin".to_vec(),
            NativeMethodEntry {
                handler: reflection_named_type_is_builtin,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_named_type_methods.insert(
            b"allowsNull".to_vec(),
            NativeMethodEntry {
                handler: reflection_type_allows_null,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_named_type_methods.insert(
            b"__toString".to_vec(),
            NativeMethodEntry {
                handler: reflection_type_to_string,
                visibility: Visibility::Public,
                is_static: false,
            },
        );

        registry.register_class(NativeClassDef {
            name: b"ReflectionNamedType".to_vec(),
            parent: Some(b"ReflectionType".to_vec()),
            is_interface: false,
            is_trait: false,
            interfaces: vec![],
            methods: reflection_named_type_methods,
            constants: HashMap::new(),
            constructor: Some(reflection_named_type_construct),
        });

        // Register ReflectionUnionType (extends ReflectionType)
        let mut reflection_union_type_methods = HashMap::new();
        
        reflection_union_type_methods.insert(
            b"__construct".to_vec(),
            NativeMethodEntry {
                handler: reflection_union_type_construct,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_union_type_methods.insert(
            b"getTypes".to_vec(),
            NativeMethodEntry {
                handler: reflection_union_type_get_types,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_union_type_methods.insert(
            b"allowsNull".to_vec(),
            NativeMethodEntry {
                handler: reflection_union_type_allows_null,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_union_type_methods.insert(
            b"__toString".to_vec(),
            NativeMethodEntry {
                handler: reflection_union_type_to_string,
                visibility: Visibility::Public,
                is_static: false,
            },
        );

        registry.register_class(NativeClassDef {
            name: b"ReflectionUnionType".to_vec(),
            parent: Some(b"ReflectionType".to_vec()),
            is_interface: false,
            is_trait: false,
            interfaces: vec![],
            methods: reflection_union_type_methods,
            constants: HashMap::new(),
            constructor: Some(reflection_union_type_construct),
        });

        // Register ReflectionIntersectionType (extends ReflectionType)
        let mut reflection_intersection_type_methods = HashMap::new();
        
        reflection_intersection_type_methods.insert(
            b"__construct".to_vec(),
            NativeMethodEntry {
                handler: reflection_intersection_type_construct,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_intersection_type_methods.insert(
            b"getTypes".to_vec(),
            NativeMethodEntry {
                handler: reflection_intersection_type_get_types,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_intersection_type_methods.insert(
            b"allowsNull".to_vec(),
            NativeMethodEntry {
                handler: reflection_intersection_type_allows_null,
                visibility: Visibility::Public,
                is_static: false,
            },
        );
        
        reflection_intersection_type_methods.insert(
            b"__toString".to_vec(),
            NativeMethodEntry {
                handler: reflection_intersection_type_to_string,
                visibility: Visibility::Public,
                is_static: false,
            },
        );

        registry.register_class(NativeClassDef {
            name: b"ReflectionIntersectionType".to_vec(),
            parent: Some(b"ReflectionType".to_vec()),
            is_interface: false,
            is_trait: false,
            interfaces: vec![],
            methods: reflection_intersection_type_methods,
            constants: HashMap::new(),
            constructor: Some(reflection_intersection_type_construct),
        });

        ExtensionResult::Success
    }

    fn request_init(&self, _context: &mut RequestContext) -> ExtensionResult {
        ExtensionResult::Success
    }

    fn request_shutdown(&self, _context: &mut RequestContext) -> ExtensionResult {
        ExtensionResult::Success
    }

    fn module_shutdown(&self) -> ExtensionResult {
        ExtensionResult::Success
    }
}
