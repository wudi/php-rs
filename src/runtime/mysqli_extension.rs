use crate::builtins::mysqli;
use crate::runtime::context::RequestContext;
use crate::runtime::extension::{Extension, ExtensionInfo, ExtensionResult};
use crate::runtime::registry::ExtensionRegistry;

/// MySQLi extension - MySQL Improved Extension
///
/// All resources (connections, results) are now managed via
/// RequestContext::resource_manager for unified, type-safe resource handling.
pub struct MysqliExtension;

impl Extension for MysqliExtension {
    fn info(&self) -> ExtensionInfo {
        ExtensionInfo {
            name: "mysqli",
            version: "1.0.0",
            dependencies: &[],
        }
    }

    fn module_init(&self, registry: &mut ExtensionRegistry) -> ExtensionResult {
        // Register MySQLi functions
        registry.register_function(b"mysqli_connect", mysqli::php_mysqli_connect);
        registry.register_function(b"mysqli_close", mysqli::php_mysqli_close);
        registry.register_function(b"mysqli_query", mysqli::php_mysqli_query);
        registry.register_function(b"mysqli_fetch_assoc", mysqli::php_mysqli_fetch_assoc);
        registry.register_function(b"mysqli_fetch_row", mysqli::php_mysqli_fetch_row);
        registry.register_function(b"mysqli_num_rows", mysqli::php_mysqli_num_rows);
        registry.register_function(b"mysqli_affected_rows", mysqli::php_mysqli_affected_rows);
        registry.register_function(b"mysqli_error", mysqli::php_mysqli_error);
        registry.register_function(b"mysqli_errno", mysqli::php_mysqli_errno);

        ExtensionResult::Success
    }

    fn module_shutdown(&self) -> ExtensionResult {
        ExtensionResult::Success
    }

    fn request_init(&self, _context: &mut RequestContext) -> ExtensionResult {
        // MySQLi resources now managed via RequestContext::resource_manager
        ExtensionResult::Success
    }

    fn request_shutdown(&self, _context: &mut RequestContext) -> ExtensionResult {
        // Cleanup is handled automatically by Drop on RequestContext
        ExtensionResult::Success
    }
}
