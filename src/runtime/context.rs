use crate::compiler::chunk::UserFunc;
use crate::core::interner::Interner;
use crate::core::value::{Handle, Symbol, Val, Visibility};
use crate::runtime::attributes::AttributeInstance;
use crate::runtime::extension::Extension;
use crate::runtime::registry::ExtensionRegistry;
use crate::runtime::resource_manager::ResourceManager;
use crate::vm::engine::VM;
use crate::vm::memory::{MemoryApi, VmHeap};
use indexmap::IndexMap;
use std::any::{Any, TypeId};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

pub type NativeHandler = fn(&mut VM, args: &[Handle]) -> Result<Handle, String>;

/// PHP configuration settings
#[derive(Debug, Clone)]
pub struct PhpConfig {
    /// Error reporting level (E_ALL = 32767)
    pub error_reporting: u32,
    /// Maximum script execution time in seconds
    pub max_execution_time: i64,
    /// Default timezone for date/time functions
    pub timezone: String,
    /// Working directory for script execution
    pub working_dir: Option<PathBuf>,
}

impl Default for PhpConfig {
    fn default() -> Self {
        Self {
            error_reporting: 32767, // E_ALL
            max_execution_time: 30,
            timezone: "UTC".to_string(),
            working_dir: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypeHint {
    Int,
    Float,
    String,
    Bool,
    Array,
    Object,
    Callable,
    Iterable,
    Mixed,
    Void,
    Never,
    Null,
    Class(Symbol),
    Union(Vec<TypeHint>),
    Intersection(Vec<TypeHint>),
}

#[derive(Debug, Clone)]
pub struct ParameterInfo {
    pub name: Symbol,
    pub type_hint: Option<TypeHint>,
    pub is_reference: bool,
    pub is_variadic: bool,
    pub default_value: Option<Val>,
    pub attributes: Vec<AttributeInstance>,
}

#[derive(Debug, Clone)]
pub struct MethodSignature {
    pub parameters: Vec<ParameterInfo>,
    pub return_type: Option<TypeHint>,
}

#[derive(Debug, Clone)]
pub struct MethodEntry {
    pub name: Symbol,
    pub func: Rc<UserFunc>,
    pub visibility: Visibility,
    pub is_static: bool,
    pub is_final: bool,
    pub declaring_class: Symbol,
    pub is_abstract: bool,
    pub signature: MethodSignature,
    pub attributes: Vec<AttributeInstance>,
}

#[derive(Debug, Clone)]
pub struct NativeMethodEntry {
    pub name: Symbol,
    pub handler: NativeHandler,
    pub visibility: Visibility,
    pub is_static: bool,
    pub is_final: bool,
    pub declaring_class: Symbol,
}

#[derive(Debug, Clone)]
pub struct TraitAliasInfo {
    pub trait_name: Option<Symbol>,
    pub method_name: Symbol,
    pub visibility: Option<Visibility>,
}

#[derive(Debug, Clone)]
pub struct PropertyEntry {
    pub default_value: Val,
    pub visibility: Visibility,
    pub type_hint: Option<TypeHint>,
    pub is_readonly: bool,
    pub attributes: Vec<AttributeInstance>,
    pub doc_comment: Option<Rc<Vec<u8>>>,
}

#[derive(Debug, Clone)]
pub struct StaticPropertyEntry {
    pub value: Val,
    pub visibility: Visibility,
    pub type_hint: Option<TypeHint>,
    pub doc_comment: Option<Rc<Vec<u8>>>,
}

#[derive(Debug, Clone)]
pub struct ClassDef {
    pub name: Symbol,
    pub parent: Option<Symbol>,
    pub is_interface: bool,
    pub is_trait: bool,
    pub is_abstract: bool,
    pub is_final: bool,
    pub is_readonly: bool,
    pub is_enum: bool,
    pub enum_backed_type: Option<EnumBackedType>,
    pub interfaces: Vec<Symbol>,
    pub traits: Vec<Symbol>,
    pub trait_aliases: HashMap<Symbol, TraitAliasInfo>,
    pub methods: HashMap<Symbol, MethodEntry>,
    pub properties: IndexMap<Symbol, PropertyEntry>, // Instance properties with type hints
    pub constants: HashMap<Symbol, (Val, Visibility)>,
    pub constant_attributes: HashMap<Symbol, Vec<AttributeInstance>>,
    pub constant_doc_comments: HashMap<Symbol, Rc<Vec<u8>>>,
    pub static_properties: HashMap<Symbol, StaticPropertyEntry>, // Static properties with type hints
    pub abstract_methods: HashSet<Symbol>,
    pub attributes: Vec<AttributeInstance>,
    pub allows_dynamic_properties: bool, // Set by #[AllowDynamicProperties] attribute
    pub doc_comment: Option<Rc<Vec<u8>>>,
    pub file_name: Option<Rc<Vec<u8>>>,
    pub start_line: Option<u32>,
    pub end_line: Option<u32>,
    pub is_internal: bool,
    pub extension_name: Option<Symbol>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnumBackedType {
    Int,
    String,
}

#[derive(Debug, Clone)]
pub struct HeaderEntry {
    pub key: Option<Vec<u8>>, // Normalized lowercase header name
    pub line: Vec<u8>,        // Original header line bytes
}

#[derive(Debug, Clone)]
pub struct ErrorInfo {
    pub error_type: i64,
    pub message: String,
    pub file: String,
    pub line: i64,
}

pub struct EngineContext {
    pub registry: ExtensionRegistry,
}

impl EngineContext {
    pub fn new() -> Self {
        let mut registry = ExtensionRegistry::new();

        // Register Core extension (MUST BE FIRST - contains all built-in functions)
        use crate::runtime::core_extension::CoreExtension;
        registry
            .register_extension(Box::new(CoreExtension))
            .expect("Failed to register Core extension");

        use crate::runtime::date_extension::DateExtension;
        registry
            .register_extension(Box::new(DateExtension))
            .expect("Failed to register Date extension");

        // Register Hash extension
        use crate::runtime::hash_extension::HashExtension;
        registry
            .register_extension(Box::new(HashExtension))
            .expect("Failed to register Hash extension");

        // Register JSON extension
        use crate::runtime::json_extension::JsonExtension;
        registry
            .register_extension(Box::new(JsonExtension))
            .expect("Failed to register JSON extension");

        // Register MySQLi extension
        use crate::runtime::mysqli_extension::MysqliExtension;
        registry
            .register_extension(Box::new(MysqliExtension))
            .expect("Failed to register MySQLi extension");

        // Register PDO extension
        use crate::runtime::pdo_extension::PdoExtension;
        registry
            .register_extension(Box::new(PdoExtension))
            .expect("Failed to register PDO extension");

        // Register Zlib extension
        use crate::runtime::zlib_extension::ZlibExtension;
        registry
            .register_extension(Box::new(ZlibExtension))
            .expect("Failed to register Zlib extension");

        // Register MBString extension
        use crate::runtime::mb_extension::MbStringExtension;
        registry
            .register_extension(Box::new(MbStringExtension))
            .expect("Failed to register mbstring extension");

        // Register Zip extension
        use crate::runtime::zip_extension::ZipExtension;
        registry
            .register_extension(Box::new(ZipExtension))
            .expect("Failed to register Zip extension");

        // Register OpenSSL extension
        use crate::runtime::openssl_extension::OpenSSLExtension;
        registry
            .register_extension(Box::new(OpenSSLExtension))
            .expect("Failed to register OpenSSL extension");

        Self { registry }
    }
}

pub struct RequestContext {
    pub engine: Arc<EngineContext>,
    pub config: PhpConfig,
    pub globals: HashMap<Symbol, Handle>,
    pub constants: HashMap<Symbol, Val>,
    pub function_attributes: HashMap<Symbol, Vec<AttributeInstance>>,
    pub user_functions: HashMap<Symbol, Rc<UserFunc>>,
    pub classes: HashMap<Symbol, ClassDef>,
    pub included_files: HashSet<String>,
    pub autoloaders: Vec<Handle>,
    pub interner: Interner,
    pub last_error: Option<ErrorInfo>,
    pub headers: Vec<HeaderEntry>,
    pub http_status: Option<i64>,
    pub native_methods: HashMap<(Symbol, Symbol), NativeMethodEntry>,
    pub next_resource_id: u64,
    /// Generic extension data storage keyed by TypeId
    pub extension_data: HashMap<TypeId, Box<dyn Any>>,
    /// Unified resource manager for type-safe resource handling
    pub resource_manager: ResourceManager,
    /// Public memory allocation API for extensions
    pub memory_api: MemoryApi,
}

impl RequestContext {
    pub fn new(engine: Arc<EngineContext>) -> Self {
        Self::with_config(engine, PhpConfig::default())
    }

    pub fn with_config(engine: Arc<EngineContext>, config: PhpConfig) -> Self {
        let mut ctx = Self {
            engine: Arc::clone(&engine),
            config,
            globals: HashMap::new(),
            constants: HashMap::new(),
            function_attributes: HashMap::new(),
            user_functions: HashMap::new(),
            classes: HashMap::new(),
            included_files: HashSet::new(),
            autoloaders: Vec::new(),
            interner: Interner::new(),
            last_error: None,
            headers: Vec::new(),
            http_status: None,
            native_methods: HashMap::new(),
            next_resource_id: 1,
            extension_data: HashMap::new(),
            resource_manager: ResourceManager::new(),
            memory_api: MemoryApi::new_unbound(),
        };

        // Copy constants from extension registry in bulk
        ctx.copy_engine_constants();

        // Materialize classes from extensions
        ctx.materialize_extension_classes();

        // Call RINIT for all extensions
        engine.registry.invoke_request_init(&mut ctx).ok();

        ctx
    }

    /// Copy constants from engine registry in bulk
    ///
    /// Two-phase constant initialization:
    /// 1. Copy extension-provided constants from engine registry (bulk operation)
    /// 2. Register core PHP constants (version info, error levels, system constants)
    ///
    /// This split is necessary because:
    /// - Extension constants are registered during MINIT at engine startup
    /// - Core PHP constants (PHP_VERSION, E_ERROR, etc.) must exist in every request
    /// - Bulk copy from registry is O(n), avoiding individual re-insertion overhead
    ///
    /// Performance: O(n) where n = number of engine constants
    fn copy_engine_constants(&mut self) {
        // Phase 1: Copy all extension constants (O(n) bulk operation)
        for (name, entry) in self.engine.registry.constants() {
            let sym = self.interner.intern(name);
            self.constants.insert(sym, entry.value.clone());
        }

        // Phase 2: Register fundamental PHP constants
        self.register_builtin_constants();
    }

    fn materialize_extension_classes(&mut self) {
        let native_classes: Vec<_> = self.engine.registry.classes().values().cloned().collect();
        for native_class in native_classes {
            let class_sym = self.interner.intern(&native_class.name);
            let parent_sym = native_class
                .parent
                .as_ref()
                .map(|p| self.interner.intern(p));
            let mut interfaces = Vec::new();
            for iface in &native_class.interfaces {
                interfaces.push(self.interner.intern(iface));
            }

            let mut constants = HashMap::new();
            for (name, (val, visibility)) in &native_class.constants {
                constants.insert(self.interner.intern(name), (val.clone(), *visibility));
            }

            let extension_name = native_class
                .extension_name
                .as_ref()
                .map(|name| self.interner.intern(name));

            self.classes.insert(
                class_sym,
                ClassDef {
                    name: class_sym,
                    parent: parent_sym,
                    is_interface: native_class.is_interface,
                    is_trait: native_class.is_trait,
                    is_abstract: false,
                    is_final: native_class.is_final,
                    is_readonly: false,
                    is_enum: false,
                    enum_backed_type: None,
                    interfaces,
                    traits: Vec::new(),
                    trait_aliases: HashMap::new(),
                    methods: HashMap::new(),
                    properties: IndexMap::new(),
                    constants,
                    constant_attributes: HashMap::new(),
                    constant_doc_comments: HashMap::new(),
                    static_properties: HashMap::new(),
                    abstract_methods: HashSet::new(),
                    attributes: Vec::new(),
                    allows_dynamic_properties: true,
                    doc_comment: None,
                    file_name: None,
                    start_line: None,
                    end_line: None,
                    is_internal: true,
                    extension_name,
                },
            );

            for (name, native_method) in &native_class.methods {
                let method_lc = name.to_ascii_lowercase();
                let method_sym = self.interner.intern(&method_lc);
                self.native_methods.insert(
                    (class_sym, method_sym),
                    NativeMethodEntry {
                        name: method_sym,
                        handler: native_method.handler,
                        visibility: native_method.visibility,
                        is_static: native_method.is_static,
                        is_final: native_method.is_final,
                        declaring_class: class_sym,
                    },
                );
            }
        }
    }

    /// Get immutable reference to extension-specific data
    ///
    /// # Type Safety
    /// Each extension should use its own unique type (e.g., JsonExtensionData)
    /// to avoid collisions in the TypeId-based storage.
    ///
    /// # Returns
    /// - `Some(&T)` if data of type T exists
    /// - `None` if no data of type T has been stored
    pub fn get_extension_data<T: 'static>(&self) -> Option<&T> {
        self.extension_data
            .get(&TypeId::of::<T>())
            .and_then(|boxed| boxed.downcast_ref::<T>())
    }

    /// Get mutable reference to extension-specific data
    ///
    /// # Type Safety
    /// Each extension should use its own unique type (e.g., JsonExtensionData)
    /// to avoid collisions in the TypeId-based storage.
    ///
    /// # Returns
    /// - `Some(&mut T)` if data of type T exists
    /// - `None` if no data of type T has been stored
    pub fn get_extension_data_mut<T: 'static>(&mut self) -> Option<&mut T> {
        self.extension_data
            .get_mut(&TypeId::of::<T>())
            .and_then(|boxed| boxed.downcast_mut::<T>())
    }

    /// Store extension-specific data
    ///
    /// # Type Safety
    /// Each extension should use its own unique type (e.g., JsonExtensionData)
    /// to avoid collisions in the TypeId-based storage.
    ///
    /// # Example
    /// ```rust,ignore
    /// struct MyExtensionData {
    ///     counter: u32,
    /// }
    /// ctx.set_extension_data(MyExtensionData { counter: 0 });
    /// ```
    pub fn set_extension_data<T: 'static>(&mut self, data: T) {
        self.extension_data
            .insert(TypeId::of::<T>(), Box::new(data));
    }

    pub fn bind_memory_api(&mut self, heap: &mut VmHeap) {
        self.memory_api.bind(heap);
    }

    pub fn alloc_bytes(&mut self, len: usize) -> crate::vm::memory::MemoryBlock {
        self.memory_api.alloc_bytes(len)
    }

    /// Get or initialize extension-specific data
    ///
    /// If data of type T does not exist, initialize it using the provided closure.
    /// Returns a mutable reference to the data (existing or newly initialized).
    ///
    /// # Type Safety
    /// Each extension should use its own unique type (e.g., JsonExtensionData)
    /// to avoid collisions in the TypeId-based storage.
    ///
    /// # Example
    /// ```rust,ignore
    /// let data = ctx.get_or_init_extension_data(|| MyExtensionData { counter: 0 });
    /// data.counter += 1;
    /// ```
    pub fn get_or_init_extension_data<T: 'static, F>(&mut self, init: F) -> &mut T
    where
        F: FnOnce() -> T,
    {
        self.extension_data
            .entry(TypeId::of::<T>())
            .or_insert_with(|| Box::new(init()))
            .downcast_mut::<T>()
            .expect("TypeId mismatch in extension_data")
    }
}

impl RequestContext {
    /// Register core PHP constants that are not provided by extensions
    ///
    /// This method only registers fundamental PHP constants that must exist
    /// in every request context. Extension-specific constants (output control,
    /// URL parsing, date formats, string functions, etc.) are registered by
    /// their respective extensions via ExtensionRegistry.
    ///
    /// Core constants registered here:
    /// - PHP version info (PHP_VERSION, PHP_VERSION_ID, etc.)
    /// - System constants (PHP_OS, PHP_SAPI, PHP_EOL)
    /// - Path separators (DIRECTORY_SEPARATOR, PATH_SEPARATOR)
    /// - Error reporting levels (E_ERROR, E_WARNING, etc.)
    fn register_builtin_constants(&mut self) {
        // PHP version constants
        const PHP_VERSION_STR: &str = "8.2.0";
        const PHP_VERSION_ID_VALUE: i64 = 80200;
        const PHP_MAJOR: i64 = 8;
        const PHP_MINOR: i64 = 2;
        const PHP_RELEASE: i64 = 0;

        self.insert_builtin_constant(
            b"PHP_VERSION",
            Val::String(Rc::new(PHP_VERSION_STR.as_bytes().to_vec())),
        );
        self.insert_builtin_constant(b"PHP_VERSION_ID", Val::Int(PHP_VERSION_ID_VALUE));
        self.insert_builtin_constant(b"PHP_MAJOR_VERSION", Val::Int(PHP_MAJOR));
        self.insert_builtin_constant(b"PHP_MINOR_VERSION", Val::Int(PHP_MINOR));
        self.insert_builtin_constant(b"PHP_RELEASE_VERSION", Val::Int(PHP_RELEASE));
        self.insert_builtin_constant(b"PHP_EXTRA_VERSION", Val::String(Rc::new(Vec::new())));

        // System constants
        self.insert_builtin_constant(b"PHP_OS", Val::String(Rc::new(b"Darwin".to_vec())));
        self.insert_builtin_constant(b"PHP_SAPI", Val::String(Rc::new(b"cli".to_vec())));
        self.insert_builtin_constant(b"PHP_EOL", Val::String(Rc::new(b"\n".to_vec())));

        // Path separator constants
        let dir_sep = std::path::MAIN_SEPARATOR.to_string().into_bytes();
        self.insert_builtin_constant(b"DIRECTORY_SEPARATOR", Val::String(Rc::new(dir_sep)));

        let path_sep_byte = if cfg!(windows) { b';' } else { b':' };
        self.insert_builtin_constant(b"PATH_SEPARATOR", Val::String(Rc::new(vec![path_sep_byte])));

        // Error reporting level constants
        self.insert_builtin_constant(b"E_ERROR", Val::Int(1));
        self.insert_builtin_constant(b"E_WARNING", Val::Int(2));
        self.insert_builtin_constant(b"E_PARSE", Val::Int(4));
        self.insert_builtin_constant(b"E_NOTICE", Val::Int(8));
        self.insert_builtin_constant(b"E_CORE_ERROR", Val::Int(16));
        self.insert_builtin_constant(b"E_CORE_WARNING", Val::Int(32));
        self.insert_builtin_constant(b"E_COMPILE_ERROR", Val::Int(64));
        self.insert_builtin_constant(b"E_COMPILE_WARNING", Val::Int(128));
        self.insert_builtin_constant(b"E_USER_ERROR", Val::Int(256));
        self.insert_builtin_constant(b"E_USER_WARNING", Val::Int(512));
        self.insert_builtin_constant(b"E_USER_NOTICE", Val::Int(1024));
        self.insert_builtin_constant(b"E_STRICT", Val::Int(2048));
        self.insert_builtin_constant(b"E_RECOVERABLE_ERROR", Val::Int(4096));
        self.insert_builtin_constant(b"E_DEPRECATED", Val::Int(8192));
        self.insert_builtin_constant(b"E_USER_DEPRECATED", Val::Int(16384));
        self.insert_builtin_constant(b"E_ALL", Val::Int(32767));
    }

    pub fn insert_builtin_constant(&mut self, name: &[u8], value: Val) {
        let sym = self.interner.intern(name);
        self.constants.insert(sym, value);
    }
}

/// Builder for constructing EngineContext with extensions
///
/// # Example
/// ```ignore
/// let engine = EngineBuilder::new()
///     .with_core_extensions()
///     .build()?;
/// ```
pub struct EngineBuilder {
    extensions: Vec<Box<dyn Extension>>,
}

impl EngineBuilder {
    /// Create a new empty builder
    pub fn new() -> Self {
        Self {
            extensions: Vec::new(),
        }
    }

    /// Add an extension to the builder
    pub fn with_extension<E: Extension + 'static>(mut self, ext: E) -> Self {
        self.extensions.push(Box::new(ext));
        self
    }

    /// Add core extensions (standard builtins)
    ///
    /// This includes all core PHP functionality: core functions, classes, interfaces,
    /// exceptions, and the date/time extension.
    pub fn with_core_extensions(mut self) -> Self {
        self.extensions
            .push(Box::new(super::core_extension::CoreExtension));
        self.extensions
            .push(Box::new(super::date_extension::DateExtension));
        self.extensions
            .push(Box::new(super::hash_extension::HashExtension));
        self.extensions
            .push(Box::new(super::mysqli_extension::MysqliExtension));
        self.extensions
            .push(Box::new(super::json_extension::JsonExtension));
        self.extensions
            .push(Box::new(super::openssl_extension::OpenSSLExtension));
        self.extensions
            .push(Box::new(super::pdo_extension::PdoExtension));
        self.extensions
            .push(Box::new(super::pthreads_extension::PthreadsExtension));
        self.extensions
            .push(Box::new(super::zlib_extension::ZlibExtension));
        self.extensions
            .push(Box::new(super::mb_extension::MbStringExtension));
        self.extensions
            .push(Box::new(crate::builtins::reflection::ReflectionExtension));
        self
    }

    /// Build the EngineContext
    ///
    /// This will:
    /// 1. Create an empty registry
    /// 2. Register all extensions (calling MINIT for each)
    /// 3. Return the configured EngineContext
    pub fn build(self) -> Result<Arc<EngineContext>, String> {
        let mut registry = ExtensionRegistry::new();

        // Register all extensions
        for ext in self.extensions {
            registry.register_extension(ext)?;
        }

        Ok(Arc::new(EngineContext { registry }))
    }
}

impl Default for EngineBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Call RSHUTDOWN for all extensions when request ends
impl Drop for RequestContext {
    fn drop(&mut self) {
        // Call RSHUTDOWN for all extensions in reverse order (LIFO)
        // Clone Arc to separate lifetimes and avoid borrow checker conflict
        let engine = Arc::clone(&self.engine);
        engine.registry.request_shutdown_all(self);
    }
}

/// Call MSHUTDOWN for all extensions when engine shuts down
impl Drop for EngineContext {
    fn drop(&mut self) {
        // Call MSHUTDOWN for all extensions in reverse order (LIFO)
        self.registry.module_shutdown_all();
    }
}
