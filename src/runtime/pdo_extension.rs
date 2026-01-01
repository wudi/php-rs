use crate::builtins::pdo;
use crate::runtime::context::RequestContext;
use crate::runtime::extension::{Extension, ExtensionInfo, ExtensionResult};
use crate::runtime::registry::ExtensionRegistry;
use std::sync::Arc;

/// Extension-specific data for PDO module
///
/// Resources (connections, statements) are managed via RequestContext::resource_manager.
/// Only the driver registry is stored here as it's module-level state, not per-resource.
#[derive(Debug)]
pub struct PdoExtensionData {
    pub driver_registry: Arc<pdo::drivers::DriverRegistry>,
}

impl Default for PdoExtensionData {
    fn default() -> Self {
        Self {
            driver_registry: Arc::new(pdo::drivers::DriverRegistry::new()),
        }
    }
}

/// PDO extension - PHP Data Objects
pub struct PdoExtension;

impl Extension for PdoExtension {
    fn info(&self) -> ExtensionInfo {
        ExtensionInfo {
            name: "pdo",
            version: "1.0.0",
            dependencies: &[],
        }
    }

    fn module_init(&self, registry: &mut ExtensionRegistry) -> ExtensionResult {
        pdo::register_pdo_extension_to_registry(registry);
        // PDO driver registry is initialized per-request in request_init
        ExtensionResult::Success
    }

    fn module_shutdown(&self) -> ExtensionResult {
        ExtensionResult::Success
    }

    fn request_init(&self, context: &mut RequestContext) -> ExtensionResult {
        // Initialize per-request PDO connections and statements
        context.set_extension_data(PdoExtensionData::default());
        ExtensionResult::Success
    }

    fn request_shutdown(&self, _context: &mut RequestContext) -> ExtensionResult {
        ExtensionResult::Success
    }
}
