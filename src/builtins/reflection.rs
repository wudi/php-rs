//! Reflection Extension - PHP Reflection API Implementation
//!
//! Reference: $PHP_SRC_PATH/ext/reflection/
//! Reference: $PHP_SRC_PATH/Zend/zend_reflection.c

use crate::core::value::{ArrayData, ArrayKey, Handle, Symbol, Val, Visibility};
use crate::runtime::context::{ClassDef, MethodEntry, ParameterInfo, RequestContext, TypeHint};
use crate::vm::engine::VM;
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
    
    // Check if class has final modifier (we need to add this to ClassDef)
    // For now, return false as placeholder
    Ok(vm.arena.alloc(Val::Bool(false)))
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
        // TODO: Create a new ReflectionClass instance for parent
        Ok(vm.arena.alloc(Val::String(Rc::new(parent_name))))
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

        registry.register_class(NativeClassDef {
            name: b"ReflectionClass".to_vec(),
            parent: None,
            is_interface: false,
            is_trait: false,
            interfaces: vec![],
            methods: reflection_class_methods,
            constants: HashMap::new(),
            constructor: Some(reflection_class_construct),
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
