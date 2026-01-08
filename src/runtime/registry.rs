use super::context::{NativeHandler, RequestContext};
use super::extension::{Extension, ExtensionInfo, ExtensionResult};
use crate::core::value::{Val, Visibility};
use std::collections::HashMap;

/// Native class definition for extension-provided classes
#[derive(Debug, Clone)]
pub struct NativeClassDef {
    pub name: Vec<u8>,
    pub parent: Option<Vec<u8>>,
    pub is_interface: bool,
    pub is_trait: bool,
    pub is_final: bool,
    pub interfaces: Vec<Vec<u8>>,
    pub methods: HashMap<Vec<u8>, NativeMethodEntry>,
    pub constants: HashMap<Vec<u8>, (Val, Visibility)>,
    pub constructor: Option<NativeHandler>,
    pub extension_name: Option<Vec<u8>>,
}

/// Native method entry for extension-provided class methods
#[derive(Debug, Clone)]
pub struct NativeMethodEntry {
    pub handler: NativeHandler,
    pub visibility: Visibility,
    pub is_static: bool,
    pub is_final: bool,
}

/// Native function entry for extension-provided functions
#[derive(Debug, Clone)]
pub struct NativeFunctionEntry {
    pub name: Vec<u8>,
    pub handler: NativeHandler,
    pub by_ref: Vec<usize>,
    pub extension_name: Option<Vec<u8>>,
}

/// Native constant entry for extension-provided constants
#[derive(Debug, Clone)]
pub struct NativeConstantEntry {
    pub value: Val,
    pub extension_name: Option<Vec<u8>>,
}

/// Extension registry - manages all loaded extensions and their registered components
///
/// This is stored in `EngineContext` and persists for the lifetime of the process
/// (or worker in FPM). It holds all extension-registered functions, classes, and constants.
pub struct ExtensionRegistry {
    /// Native function entries (name -> entry)
    functions: HashMap<Vec<u8>, NativeFunctionEntry>,
    /// Native class definitions (name -> class def)
    classes: HashMap<Vec<u8>, NativeClassDef>,
    /// Registered extensions
    extensions: Vec<Box<dyn Extension>>,
    /// Extension name -> index mapping for fast lookup
    extension_map: HashMap<String, usize>,
    /// Engine-level constants (name -> entry)
    constants: HashMap<Vec<u8>, NativeConstantEntry>,
    /// Currently registering extension name for tagging native components
    current_extension_name: Option<Vec<u8>>,
}

impl ExtensionRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            functions: HashMap::new(),
            classes: HashMap::new(),
            extensions: Vec::new(),
            extension_map: HashMap::new(),
            constants: HashMap::new(),
            current_extension_name: None,
        }
    }

    /// Register a native function handler
    ///
    /// Function names are stored with lowercase keys for case-insensitive O(1) lookup,
    /// while the original name is preserved in the entry.
    pub fn register_function(&mut self, name: &[u8], handler: NativeHandler) {
        let name_vec = name.to_vec();
        let lower_name = name_vec.to_ascii_lowercase();
        self.functions.insert(
            lower_name,
            NativeFunctionEntry {
                name: name_vec,
                handler,
                by_ref: Vec::new(),
                extension_name: self.current_extension_name.clone(),
            },
        );
    }

    /// Register a native function handler with by-ref argument positions.
    pub fn register_function_with_by_ref(
        &mut self,
        name: &[u8],
        handler: NativeHandler,
        by_ref: Vec<usize>,
    ) {
        let name_vec = name.to_vec();
        let lower_name = name_vec.to_ascii_lowercase();
        self.functions.insert(
            lower_name,
            NativeFunctionEntry {
                name: name_vec,
                handler,
                by_ref,
                extension_name: self.current_extension_name.clone(),
            },
        );
    }

    /// Register a native class definition
    pub fn register_class(&mut self, class: NativeClassDef) {
        let mut class = class;
        if class.extension_name.is_none() {
            class.extension_name = self.current_extension_name.clone();
        }
        self.classes.insert(class.name.clone(), class);
    }

    /// Register an engine-level constant
    ///
    /// Constant names are stored as byte slices and later interned when needed.
    pub fn register_constant(&mut self, name: &[u8], value: Val) {
        self.constants.insert(
            name.to_vec(),
            NativeConstantEntry {
                value,
                extension_name: self.current_extension_name.clone(),
            },
        );
    }

    /// Get a function handler by name (case-insensitive lookup)
    pub fn get_function(&self, name: &[u8]) -> Option<NativeHandler> {
        // Try exact match first (useful if name is already lowercased by caller)
        if let Some(entry) = self.functions.get(name) {
            return Some(entry.handler);
        }

        // Fallback to case-insensitive lookup
        let lower_name = name.to_ascii_lowercase();
        self.functions.get(&lower_name).map(|entry| entry.handler)
    }

    /// Get by-ref argument indexes for a function (case-insensitive lookup)
    pub fn get_function_by_ref(&self, name: &[u8]) -> Option<&[usize]> {
        if let Some(entry) = self.functions.get(name) {
            return Some(entry.by_ref.as_slice());
        }

        let lower_name = name.to_ascii_lowercase();
        self.functions
            .get(&lower_name)
            .map(|entry| entry.by_ref.as_slice())
    }

    /// Get a class definition by name
    pub fn get_class(&self, name: &[u8]) -> Option<&NativeClassDef> {
        self.classes.get(name)
    }

    /// Get an engine-level constant by name (case-sensitive)
    pub fn get_constant(&self, name: &[u8]) -> Option<&Val> {
        self.constants.get(name).map(|e| &e.value)
    }

    /// Check if an extension is loaded
    pub fn extension_loaded(&self, name: &str) -> bool {
        self.extension_map.contains_key(name)
    }

    /// Get list of all loaded extension names
    pub fn get_extensions(&self) -> Vec<&str> {
        self.extension_map.keys().map(|s| s.as_str()).collect()
    }

    /// Get extension metadata by name (case-insensitive).
    pub fn get_extension_info_by_name_ci(&self, name: &str) -> Option<ExtensionInfo> {
        for (ext_name, &index) in &self.extension_map {
            if ext_name.eq_ignore_ascii_case(name) {
                if let Some(ext) = self.extensions.get(index) {
                    return Some(ext.info());
                }
            }
        }
        None
    }

    /// Register an extension and call its MINIT hook
    ///
    /// Returns an error if:
    /// - Extension with same name already registered
    /// - Dependencies are not satisfied
    /// - MINIT hook fails
    pub fn register_extension(&mut self, extension: Box<dyn Extension>) -> Result<(), String> {
        let info = extension.info();

        // Check if already registered
        if self.extension_map.contains_key(info.name) {
            return Err(format!("Extension '{}' is already registered", info.name));
        }

        // Check dependencies
        for &dep in info.dependencies {
            if !self.extension_map.contains_key(dep) {
                return Err(format!(
                    "Extension '{}' depends on '{}' which is not loaded",
                    info.name, dep
                ));
            }
        }

        let previous_extension_name = self.current_extension_name.take();
        self.current_extension_name = Some(info.name.as_bytes().to_vec());

        // Call MINIT
        let init_result = extension.module_init(self);

        self.current_extension_name = previous_extension_name;

        match init_result {
            ExtensionResult::Success => {
                let index = self.extensions.len();
                self.extension_map.insert(info.name.to_string(), index);
                self.extensions.push(extension);
                Ok(())
            }
            ExtensionResult::Failure(msg) => {
                Err(format!("Extension '{}' MINIT failed: {}", info.name, msg))
            }
        }
    }

    /// Call RINIT on all registered extensions for request initialization
    pub fn request_init_all(&self, context: &mut crate::runtime::context::RequestContext) {
        for ext in &self.extensions {
            if let ExtensionResult::Failure(msg) = ext.request_init(context) {
                eprintln!(
                    "Warning: Extension '{}' RINIT failed: {}",
                    ext.info().name,
                    msg
                );
            }
        }
    }

    /// Call RSHUTDOWN on all registered extensions for request cleanup
    pub fn request_shutdown_all(&self, context: &mut crate::runtime::context::RequestContext) {
        // Call in reverse order (LIFO) for proper cleanup
        for ext in self.extensions.iter().rev() {
            if let ExtensionResult::Failure(msg) = ext.request_shutdown(context) {
                eprintln!(
                    "Warning: Extension '{}' RSHUTDOWN failed: {}",
                    ext.info().name,
                    msg
                );
            }
        }
    }

    /// Call module_shutdown on all registered extensions (called on engine drop)
    pub fn module_shutdown_all(&mut self) {
        // Call in reverse order (LIFO) for proper cleanup
        for ext in self.extensions.iter_mut().rev() {
            if let ExtensionResult::Failure(msg) = ext.module_shutdown() {
                eprintln!(
                    "Warning: Extension '{}' MSHUTDOWN failed: {}",
                    ext.info().name,
                    msg
                );
            }
        }
    }

    /// Invoke RINIT for all extensions
    pub fn invoke_request_init(&self, context: &mut RequestContext) -> Result<(), String> {
        for ext in &self.extensions {
            match ext.request_init(context) {
                ExtensionResult::Success => {}
                ExtensionResult::Failure(msg) => {
                    return Err(format!(
                        "Extension '{}' RINIT failed: {}",
                        ext.info().name,
                        msg
                    ));
                }
            }
        }
        Ok(())
    }

    /// Invoke RSHUTDOWN for all extensions (in reverse order)
    pub fn invoke_request_shutdown(&self, context: &mut RequestContext) -> Result<(), String> {
        for ext in self.extensions.iter().rev() {
            match ext.request_shutdown(context) {
                ExtensionResult::Success => {}
                ExtensionResult::Failure(msg) => {
                    return Err(format!(
                        "Extension '{}' RSHUTDOWN failed: {}",
                        ext.info().name,
                        msg
                    ));
                }
            }
        }
        Ok(())
    }

    /// Invoke MSHUTDOWN for all extensions (in reverse order)
    pub fn invoke_module_shutdown(&self) -> Result<(), String> {
        for ext in self.extensions.iter().rev() {
            match ext.module_shutdown() {
                ExtensionResult::Success => {}
                ExtensionResult::Failure(msg) => {
                    return Err(format!(
                        "Extension '{}' MSHUTDOWN failed: {}",
                        ext.info().name,
                        msg
                    ));
                }
            }
        }
        Ok(())
    }

    /// Get all registered functions
    pub fn functions(&self) -> &HashMap<Vec<u8>, NativeFunctionEntry> {
        &self.functions
    }

    /// Get all registered constants
    pub fn constants(&self) -> &HashMap<Vec<u8>, NativeConstantEntry> {
        &self.constants
    }

    /// Get all registered classes
    pub fn classes(&self) -> &HashMap<Vec<u8>, NativeClassDef> {
        &self.classes
    }

    /// Get functions belonging to a specific extension
    pub fn get_functions_by_extension(
        &self,
        extension_name: &[u8],
    ) -> Vec<(&[u8], &NativeFunctionEntry)> {
        self.functions
            .iter()
            .filter(|(_, entry)| {
                entry
                    .extension_name
                    .as_ref()
                    .map(|n| n.as_slice() == extension_name)
                    .unwrap_or(false)
            })
            .map(|(_, entry)| (entry.name.as_slice(), entry))
            .collect()
    }

    /// Get constants belonging to a specific extension
    pub fn get_constants_by_extension(
        &self,
        extension_name: &[u8],
    ) -> Vec<(&[u8], &NativeConstantEntry)> {
        self.constants
            .iter()
            .filter(|(_, entry)| {
                entry
                    .extension_name
                    .as_ref()
                    .map(|n| n.as_slice() == extension_name)
                    .unwrap_or(false)
            })
            .map(|(name, entry)| (name.as_slice(), entry))
            .collect()
    }
}

impl Default for ExtensionRegistry {
    fn default() -> Self {
        Self::new()
    }
}
