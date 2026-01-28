use crate::compiler::emitter::Emitter;
use crate::parser::lexer::Lexer;
use crate::parser::parser::Parser as PhpParser;
use crate::phpt::matcher::{match_output, ExpectationType};
use crate::phpt::output_writer::BufferedOutputWriter;
use crate::phpt::parser::PhptTest;
use crate::runtime::context::{EngineBuilder, EngineContext};
use crate::vm::engine::{OutputWriter, VM};
use bumpalo::Bump;
use std::rc::Rc;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub enum TestResult {
    Passed,
    Failed { expected: String, actual: String },
    Skipped { reason: String },
    Error { error: String },
}

pub struct PhptExecutor {
    engine_context: Arc<EngineContext>,
}

impl PhptExecutor {
    pub fn new() -> Result<Self, String> {
        let builder = EngineBuilder::new();
        let engine_context = builder
            .with_core_extensions()
            .build()
            .map_err(|e| format!("Failed to build engine: {}", e))?;

        Ok(Self { engine_context })
    }

    pub fn run_test(&self, test: &PhptTest) -> TestResult {
        // Check SKIPIF first
        if let Some(ref skipif_code) = test.sections.skipif {
            match self.execute_skipif(skipif_code) {
                Ok(Some(reason)) => return TestResult::Skipped { reason },
                Err(e) => return TestResult::Error {
                    error: format!("SKIPIF error: {}", e),
                },
                Ok(None) => {
                    // Continue with test
                }
            }
        }

        // Apply environment variables
        for (key, value) in &test.sections.env {
            unsafe {
                std::env::set_var(key, value);
            }
        }

        // Execute the test
        let result = self.execute_test_code(test);

        // Clean up environment variables
        for (key, _) in &test.sections.env {
            unsafe {
                std::env::remove_var(key);
            }
        }

        // Run CLEAN section if present
        if let Some(ref clean_code) = test.sections.clean {
            if let Err(e) = self.execute_clean(clean_code) {
                eprintln!("Warning: CLEAN section failed: {}", e);
            }
        }

        result
    }

    fn execute_skipif(&self, skipif_code: &str) -> Result<Option<String>, String> {
        let mut vm = VM::new_with_sapi(self.engine_context.clone(), crate::sapi::SapiMode::Cli);
        let output_writer = BufferedOutputWriter::new();
        vm.set_output_writer(Box::new(output_writer.clone()));

        if let Err(e) = self.execute_source(skipif_code, &mut vm, None) {
            return Err(format!("Failed to execute SKIPIF: {}", e));
        }

        let output = output_writer.get_output();

        // Check if output contains "skip"
        if output.to_lowercase().contains("skip") {
            // Extract reason (everything after "skip")
            let reason = output
                .lines()
                .find(|line| line.to_lowercase().contains("skip"))
                .and_then(|line| {
                    let lower = line.to_lowercase();
                    lower.find("skip").map(|pos| {
                        let after = &line[pos + 4..];
                        after.trim().to_string()
                    })
                })
                .unwrap_or_else(|| "Test skipped".to_string());

            Ok(Some(reason))
        } else {
            Ok(None)
        }
    }

    fn execute_test_code(&self, test: &PhptTest) -> TestResult {
        let mut vm = VM::new_with_sapi(self.engine_context.clone(), crate::sapi::SapiMode::Cli);

        // Set up output writer and error handler FIRST
        // so warnings during INI application and POST parsing are captured
        let output_writer = BufferedOutputWriter::new();
        vm.set_output_writer(Box::new(output_writer.clone()));
        
        let error_handler = crate::phpt::output_writer::BufferedErrorHandler::new(
            output_writer.state.clone()
        );
        vm.set_error_handler(Box::new(error_handler));

        // Apply INI settings from --INI-- section (may emit warnings)
        self.apply_ini_settings(&mut vm, test);

        // Set up HTTP superglobals from test sections
        self.setup_superglobals(&mut vm, test);

        // Set up $_SERVER superglobal with argv/argc
        let argv_warning = self.setup_server_superglobal(&mut vm, test);

        if let Some(warning) = argv_warning {
            let mut writer = output_writer.clone();
            // Best-effort: prepend warning so EXPECTF patterns match PHP behavior.
            let _ = writer.write(warning.as_bytes());
        }

        // Generate .php filename from test file path
        let php_filename = test.file_path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| format!("{}.php", s));
        
        if let Err(e) = self.execute_source(&test.sections.file, &mut vm, php_filename.as_deref()) {
            return TestResult::Error {
                error: format!("Execution error: {}", e),
            };
        }

        let actual_output = output_writer.get_output();

        // Determine expected output type
        let expected = if let Some(ref expect) = test.sections.expect {
            ExpectationType::Exact(expect.clone())
        } else if let Some(ref expectf) = test.sections.expectf {
            ExpectationType::Format(expectf.clone())
        } else if let Some(ref expectregex) = test.sections.expectregex {
            ExpectationType::Regex(expectregex.clone())
        } else {
            return TestResult::Error {
                error: "No EXPECT/EXPECTF/EXPECTREGEX section found".to_string(),
            };
        };

        if match_output(&actual_output, expected.clone()) {
            TestResult::Passed
        } else {
            let expected_str = match expected {
                ExpectationType::Exact(s) => s,
                ExpectationType::Format(s) => format!("EXPECTF: {}", s),
                ExpectationType::Regex(s) => format!("EXPECTREGEX: {}", s),
            };

            TestResult::Failed {
                expected: expected_str,
                actual: actual_output,
            }
        }
    }

    fn execute_clean(&self, clean_code: &str) -> Result<(), String> {
        let mut vm = VM::new_with_sapi(self.engine_context.clone(), crate::sapi::SapiMode::Cli);
        let output_writer = BufferedOutputWriter::new();
        vm.set_output_writer(Box::new(output_writer));

        self.execute_source(clean_code, &mut vm, None)
            .map_err(|e| format!("CLEAN failed: {}", e))
    }

    fn setup_superglobals(&self, vm: &mut VM, test: &PhptTest) {
        use crate::core::value::{Val, ArrayData};
        use indexmap::IndexMap;
        use std::rc::Rc;

        // Parse and set $_GET
        if let Some(ref get_data) = test.sections.get {
            let get_array = self.parse_query_string(get_data, vm);
            let get_sym = vm.context.interner.intern(b"_GET");
            vm.context.globals.insert(get_sym, get_array);
        }

        // Parse and set $_POST
        if let Some(ref post_data) = test.sections.post {
            // Check post_max_size (0 means unlimited)
            let post_max_size = self.parse_post_max_size(vm);
            
            let content_length = post_data.len();
            let exceeds_limit = post_max_size.map_or(false, |limit| content_length > limit);
            
            if exceeds_limit {
                use crate::vm::engine::ErrorLevel;
                let limit_str = post_max_size.unwrap();
                let msg = format!(
                    "PHP Request Startup: POST Content-Length of {} bytes exceeds the limit of {} bytes in Unknown",
                    content_length, limit_str
                );
                vm.report_error(ErrorLevel::Warning, &msg);
                // Set empty $_POST array
                let empty_array = vm.arena.alloc(Val::Array(Rc::new(ArrayData::from(IndexMap::new()))));
                let post_sym = vm.context.interner.intern(b"_POST");
                vm.context.globals.insert(post_sym, empty_array);
            } else {
                let post_array = self.parse_query_string(post_data, vm);
                let post_sym = vm.context.interner.intern(b"_POST");
                vm.context.globals.insert(post_sym, post_array);
            }
        }

        // Parse POST_RAW if present (multipart/form-data)
        if let Some(ref post_raw) = test.sections.post_raw {
            // Check post_max_size (0 means unlimited)
            let post_max_size = self.parse_post_max_size(vm);
            
            // Content-Length is the body size (excluding the Content-Type header line)
            let lines: Vec<&str> = post_raw.lines().collect();
            let body_content = if lines.len() > 1 {
                lines[1..].join("\n")
            } else {
                String::new()
            };
            let content_length = body_content.len();
            let exceeds_limit = post_max_size.map_or(false, |limit| content_length > limit);
            
            if exceeds_limit {
                use crate::vm::engine::ErrorLevel;
                let limit_str = post_max_size.unwrap();
                let msg = format!(
                    "PHP Request Startup: POST Content-Length of {} bytes exceeds the limit of {} bytes in Unknown",
                    content_length, limit_str
                );
                vm.report_error(ErrorLevel::Warning, &msg);
                // Don't parse POST data when size exceeded
            } else {
                self.parse_post_raw(post_raw, vm);
            }
        }

        // Parse and set $_COOKIE
        if let Some(ref cookie_data) = test.sections.cookie {
            let cookie_array = self.parse_cookie_string(cookie_data, vm);
            let cookie_sym = vm.context.interner.intern(b"_COOKIE");
            vm.context.globals.insert(cookie_sym, cookie_array);
        }

        // Set up $_REQUEST (combination of GET, POST, COOKIE)
        let mut request_map = IndexMap::new();

        let get_sym = vm.context.interner.intern(b"_GET");
        let post_sym = vm.context.interner.intern(b"_POST");
        let cookie_sym = vm.context.interner.intern(b"_COOKIE");

        // Merge arrays: GET, then POST, then COOKIE (POST overrides GET, COOKIE overrides both)
        if let Some(get_handle) = vm.context.globals.get(&get_sym) {
            if let Val::Array(arr) = &vm.arena.get(*get_handle).value {
                for (key, value) in &arr.map {
                    request_map.insert(key.clone(), *value);
                }
            }
        }
        if let Some(post_handle) = vm.context.globals.get(&post_sym) {
            if let Val::Array(arr) = &vm.arena.get(*post_handle).value {
                for (key, value) in &arr.map {
                    request_map.insert(key.clone(), *value);
                }
            }
        }
        if let Some(cookie_handle) = vm.context.globals.get(&cookie_sym) {
            if let Val::Array(arr) = &vm.arena.get(*cookie_handle).value {
                for (key, value) in &arr.map {
                    request_map.insert(key.clone(), *value);
                }
            }
        }

        let request_array = Val::Array(Rc::new(ArrayData::from(request_map)));
        let request_handle = vm.arena.alloc(request_array);
        let request_sym = vm.context.interner.intern(b"_REQUEST");
        vm.context.globals.insert(request_sym, request_handle);
    }

    fn parse_query_string(&self, query: &str, vm: &mut VM) -> crate::core::value::Handle {
        use crate::core::value::{ArrayData, Val};
        use indexmap::IndexMap;
        use std::rc::Rc;

        let mut result = IndexMap::new();
        
        // Get max_input_nesting_level (default 64)
        let max_nesting_level = vm.context.config.ini_settings.get("max_input_nesting_level")
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(64);

        for pair in query.split('&') {
            let (key, value) = if let Some(pos) = pair.find('=') {
                (&pair[..pos], &pair[pos + 1..])
            } else {
                (pair, "")
            };

            let decoded_key = Self::url_decode(key);
            if decoded_key.is_empty() {
                continue;
            }
            let decoded_value = Self::url_decode(value);

            // Convert decoded key bytes to string for parsing
            let key_str = String::from_utf8_lossy(&decoded_key).to_string();
            self.insert_query_pair(&mut result, &key_str, &decoded_value, max_nesting_level, vm);
        }

        let array_data = ArrayData::from(result);
        let array_val = Val::Array(Rc::new(array_data));
        vm.arena.alloc(array_val)
    }

    fn insert_query_pair(
        &self,
        result: &mut indexmap::IndexMap<crate::core::value::ArrayKey, crate::core::value::Handle>,
        key: &str,
        value: &[u8],
        max_nesting_level: usize,
        vm: &mut VM,
    ) {
        let (base, mut segments) = Self::parse_key_parts(key);
        if base.is_empty() {
            return;
        }

        let mut parts = Vec::with_capacity(1 + segments.len());
        parts.push(base);
        parts.append(&mut segments);
        
        // Check nesting level - parts.len() is the total depth
        if parts.len() > max_nesting_level {
            // Exceeds max nesting - discard this value
            return;
        }

        self.insert_parts(result, &parts, value, vm);
    }

    fn parse_key_parts(key: &str) -> (String, Vec<String>) {
        let mut base = String::new();
        let mut segments = Vec::new();
        let chars: Vec<char> = key.chars().collect();
        let mut i = 0;

        // Parse base name until first '['
        while i < chars.len() && chars[i] != '[' {
            base.push(chars[i]);
            i += 1;
        }

        // PHP's bracket parsing rules:
        // - Parse segments left to right
        // - '[' opens a segment, next ']' closes it (content can include '[')
        // - Extra ']' outside segments are ignored
        // - Only unclosed '[' (segment started but never closed) is malformed
        
        // Check if there's an unclosed '[' by simulating the segment parsing
        let has_trailing_unclosed = {
            let mut temp_i = i;
            let mut unclosed = false;
            while temp_i < chars.len() {
                if chars[temp_i] == '[' {
                    temp_i += 1; // Skip opening '['
                    // Find closing ']'
                    while temp_i < chars.len() && chars[temp_i] != ']' {
                        temp_i += 1;
                    }
                    if temp_i >= chars.len() {
                        // Reached end without finding ']' - unclosed!
                        unclosed = true;
                        break;
                    }
                    temp_i += 1; // Skip closing ']'
                } else {
                    temp_i += 1; // Skip other characters (including extra ']')
                }
            }
            unclosed
        };

        if has_trailing_unclosed {
            // Flatten: convert [ and spaces/dots to _
            let flattened = key.chars().map(|c| match c {
                '[' | ' ' | '.' => '_',
                _ => c
            }).collect::<String>();
            return (flattened, vec![]);
        }

        // Parse bracket segments
        let mut has_trailing_chars = false;
        while i < chars.len() {
            if chars[i] == '[' {
                i += 1; // skip opening '['
                let mut segment = String::new();

                // Collect characters until we find the closing ']'
                // Characters inside (including '[') become the key
                while i < chars.len() && chars[i] != ']' {
                    segment.push(chars[i]);
                    i += 1;
                }
                
                if i < chars.len() {
                    i += 1; // skip closing ']'
                }

                segments.push(segment);
            } else if chars[i] == ']' {
                // Extra ']' outside brackets - ignore
                i += 1;
            } else {
                // Other characters after brackets - malformed for file uploads
                has_trailing_chars = true;
                i += 1;
            }
        }

        if has_trailing_chars {
            // Return empty segments to signal malformed input
            return (base, vec![]);
        }

        (base, segments)
    }

    fn insert_parts(
        &self,
        map: &mut indexmap::IndexMap<crate::core::value::ArrayKey, crate::core::value::Handle>,
        parts: &[String],
        value: &[u8],
        vm: &mut VM,
    ) {
        use crate::core::value::{ArrayData, ArrayKey, Val};
        use std::rc::Rc;

        let is_last = parts.len() == 1;
        let part = &parts[0];

        let array_key = if part.is_empty() {
            ArrayKey::Int(self.next_index(map))
        } else if let Ok(idx) = part.parse::<i64>() {
            ArrayKey::Int(idx)
        } else {
            ArrayKey::Str(Rc::new(part.as_bytes().to_vec()))
        };

        if is_last {
            let val = Val::String(Rc::new(value.to_vec()));
            let handle = vm.arena.alloc(val);
            map.insert(array_key, handle);
            return;
        }

        let existing_handle = map.get(&array_key).copied();
        let mut child_map = if let Some(handle) = existing_handle {
            if let Val::Array(arr) = &vm.arena.get(handle).value {
                arr.map.clone()
            } else {
                indexmap::IndexMap::new()
            }
        } else {
            indexmap::IndexMap::new()
        };

        self.insert_parts(&mut child_map, &parts[1..], value, vm);

        let new_array = Val::Array(Rc::new(ArrayData::from(child_map)));
        let new_handle = vm.arena.alloc(new_array);
        map.insert(array_key, new_handle);
    }

    fn next_index(
        &self,
        map: &indexmap::IndexMap<crate::core::value::ArrayKey, crate::core::value::Handle>,
    ) -> i64 {
        let mut max = -1;
        for key in map.keys() {
            if let crate::core::value::ArrayKey::Int(idx) = key {
                if *idx > max {
                    max = *idx;
                }
            }
        }
        max + 1
    }

    fn parse_cookie_string(&self, cookie: &str, vm: &mut VM) -> crate::core::value::Handle {
        use crate::core::value::{ArrayData, ArrayKey, Val};
        use indexmap::IndexMap;
        use std::rc::Rc;

        let mut result = IndexMap::new();

        for pair in cookie.split(';') {
            let pair = pair.trim_start();  // Only trim leading whitespace
            if pair.is_empty() {
                continue;
            }
            
            let (key, value) = if let Some(pos) = pair.find('=') {
                let key = pair[..pos].trim();
                let value = &pair[pos + 1..];
                (key, value)
            } else {
                // Cookie without value (e.g., "cookie_name")
                (pair, "")
            };

            // Replace spaces and dots with underscores in key (PHP behavior)
            let normalized_key = key.chars().map(|c| {
                match c {
                    ' ' | '.' => '_',
                    _ => c
                }
            }).collect::<String>();
            
            // Cookie values use percent-encoding only (RFC 6265), not + for space
            let value_bytes = Self::cookie_decode(value);

            let array_key = ArrayKey::Str(Rc::new(normalized_key.as_bytes().to_vec()));
            
            // PHP keeps the FIRST value when there are duplicate cookie names
            if !result.contains_key(&array_key) {
                let val = Val::String(Rc::new(value_bytes));
                let val_handle = vm.arena.alloc(val);
                result.insert(array_key, val_handle);
            }
        }

        let array_data = ArrayData::from(result);
        let array_val = Val::Array(Rc::new(array_data));
        vm.arena.alloc(array_val)
    }

    fn url_decode(s: &str) -> Vec<u8> {
        let mut result = Vec::new();
        let mut chars = s.chars().peekable();

        while let Some(ch) = chars.next() {
            match ch {
                '+' => result.push(b' '),
                '%' => {
                    let hex: String = chars.by_ref().take(2).collect();
                    if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                        result.push(byte);
                    } else {
                        result.push(b'%');
                        result.extend_from_slice(hex.as_bytes());
                    }
                }
                _ => {
                    // For non-ASCII or multi-byte UTF-8, encode as bytes
                    let mut buf = [0u8; 4];
                    let encoded = ch.encode_utf8(&mut buf);
                    result.extend_from_slice(encoded.as_bytes());
                }
            }
        }

        result
    }

    /// Decode cookie values using RFC 6265 rules (percent-encoding only, + is NOT space)
    fn cookie_decode(s: &str) -> Vec<u8> {
        let mut result = Vec::new();
        let mut chars = s.chars().peekable();

        while let Some(ch) = chars.next() {
            match ch {
                '%' => {
                    let hex: String = chars.by_ref().take(2).collect();
                    if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                        result.push(byte);
                    } else {
                        result.push(b'%');
                        result.extend_from_slice(hex.as_bytes());
                    }
                }
                _ => {
                    // For non-ASCII or multi-byte UTF-8, encode as bytes
                    let mut buf = [0u8; 4];
                    let encoded = ch.encode_utf8(&mut buf);
                    result.extend_from_slice(encoded.as_bytes());
                }
            }
        }

        result
    }

    fn parse_post_raw(&self, post_raw: &str, vm: &mut VM) {
        use crate::core::value::{Val, ArrayData, ArrayKey};
        use indexmap::IndexMap;
        use std::rc::Rc;

        // Extract boundary from Content-Type header (first line)
        let lines: Vec<&str> = post_raw.lines().collect();
        if lines.is_empty() {
            return;
        }

        // First line should be Content-Type with boundary
        let boundary = if let Some(content_type_line) = lines.first() {
            if let Some(boundary_pos) = content_type_line.find("boundary=") {
                let boundary_start = boundary_pos + "boundary=".len();
                let boundary_rest = &content_type_line[boundary_start..];
                // Boundary value may be quoted or followed by semicolon or comma
                let end_pos = boundary_rest.find(';')
                    .or_else(|| boundary_rest.find(','))
                    .unwrap_or(boundary_rest.len());
                let boundary_value = &boundary_rest[..end_pos];
                // Remove quotes if present
                let boundary_trimmed = boundary_value.trim();
                
                // Check for invalid quoted boundary (opening quote without closing quote)
                if boundary_trimmed.starts_with('"') && (!boundary_trimmed.ends_with('"') || boundary_trimmed.len() == 1) {
                    use crate::vm::engine::ErrorLevel;
                    vm.report_error(ErrorLevel::Warning, "PHP Request Startup: Invalid boundary in multipart/form-data POST data in Unknown");
                    return;
                }
                
                if boundary_trimmed.starts_with('"') && boundary_trimmed.ends_with('"') && boundary_trimmed.len() > 1 {
                    &boundary_trimmed[1..boundary_trimmed.len()-1]
                } else {
                    boundary_trimmed
                }
            } else {
                // No boundary found - emit warning and return
                use crate::vm::engine::ErrorLevel;
                vm.report_error(ErrorLevel::Warning, "PHP Request Startup: Missing boundary in multipart/form-data POST data in Unknown");
                return;
            }
        } else {
            return;
        };

        // Join all lines after the first (the actual multipart body)
        let body = lines[1..].join("\n");
        
        let delimiter = format!("--{}", boundary);
        let end_delimiter = format!("--{}--", boundary);
        
        // Check if the multipart data ends with proper closing boundary
        let has_proper_ending = body.trim_end().ends_with(&end_delimiter) || 
                                 body.trim_end().ends_with(&format!("{}\r\n", end_delimiter)) ||
                                 body.trim_end().ends_with(&format!("{}\n", end_delimiter));
        
        let mut post_map = IndexMap::new();
        // Track file uploads: Vec<(field_name, name, full_path, type, tmp_name, error, size)>
        let mut file_uploads: Vec<(String, String, String, String, String, i64, i64)> = Vec::new();
        
        // Track MAX_FILE_SIZE from form data (client-side hint for file size limit)
        let mut max_file_size: Option<usize> = None;
        
        // Track if we've emitted a garbled headers warning
        let mut has_garbled_warning = false;
        
        // Get max_input_nesting_level for validation
        let max_nesting_level = vm.context.config.ini_settings.get("max_input_nesting_level")
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(64);
        
        // Get max_file_uploads limit
        let max_file_uploads = vm.context.config.ini_settings.get("max_file_uploads")
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(20);
        
        // Split by boundary
        let parts: Vec<&str> = body.split(&delimiter).collect();
        for (idx, part) in parts.iter().enumerate() {
            if part.trim().is_empty() || part.trim() == end_delimiter.trim() || part.starts_with("--") {
                continue;
            }
            
            // Check if this is the last part
            let is_last_part = idx == parts.len() - 1;
            
            // Split headers and body
            // Note: Parts often start with an empty line after the boundary delimiter
            let part_lines: Vec<&str> = part.split('\n').collect();
            
            // Skip initial empty lines
            let mut start_idx = 0;
            while start_idx < part_lines.len() && part_lines[start_idx].trim().is_empty() {
                start_idx += 1;
            }
            
            // Find the blank line separating headers from body
            let mut headers_end = start_idx;
            for i in start_idx..part_lines.len() {
                if part_lines[i].trim().is_empty() {
                    headers_end = i;
                    break;
                }
            }
            
            if start_idx >= headers_end || headers_end >= part_lines.len() {
                continue;
            }
            
            let headers = &part_lines[start_idx..headers_end];
            let body_lines = &part_lines[headers_end + 1..]; // Skip the blank line
            
            // Parse Content-Disposition header
            let mut field_name: Option<String> = None;
            let mut filename: Option<String> = None;
            let mut content_type: Option<String> = None;
            
            for header in headers {
                if header.starts_with("Content-Disposition:") {
                    // Extract name and filename
                    for segment in header.split(';') {
                        let segment = segment.trim();
                        
                        // Parse name=value, name='value', or name="value"
                        if let Some(eq_pos) = segment.find('=') {
                            let key = segment[..eq_pos].trim();
                            let value = &segment[eq_pos + 1..];
                            
                            if key == "name" {
                                field_name = Some(Self::parse_disposition_value(value));
                            } else if key == "filename" {
                                filename = Some(Self::parse_disposition_value(value));
                            }
                        }
                    }
                } else if header.starts_with("Content-Type:") {
                    let ct = header["Content-Type:".len()..].trim();
                    // Remove trailing semicolons and whitespace
                    content_type = Some(ct.trim_end_matches(';').trim().to_string());
                }
            }
            
            // Skip parts with garbled headers (no name and no filename)
            if field_name.is_none() && filename.is_none() {
                // Emit warning only once for garbled headers
                if !has_garbled_warning {
                    use crate::vm::engine::ErrorLevel;
                    vm.report_error(ErrorLevel::Warning, "PHP Request Startup: File Upload Mime headers garbled");
                    has_garbled_warning = true;
                }
                continue;
            }
            
            // For anonymous uploads (no name, only filename), use numeric index
            let name = field_name.unwrap_or_else(|| file_uploads.len().to_string());
            
            // Body content (join remaining lines)
            let mut content = body_lines.join("\n");
            // Remove only the final newline (before boundary), not all trailing whitespace
            if content.ends_with('\n') {
                content.pop();
            }
            let content_trimmed = &content;
            
            // Check file_uploads INI setting
            let file_uploads_enabled = vm.context.config.ini_settings.get("file_uploads")
                .map(|v| v != "0" && v.to_lowercase() != "off" && v.to_lowercase() != "false")
                .unwrap_or(true); // Default is enabled
            
            if let Some(fname) = filename {
                // This is a file upload
                if !file_uploads_enabled {
                    // file_uploads is disabled - skip file uploads
                    continue;
                }
                
                // Check max_file_uploads limit
                if file_uploads.len() >= max_file_uploads {
                    continue;
                }
                
                // Parse field name and check nesting level  
                let (base, mut segments) = Self::parse_key_parts(&name);
                if base.is_empty() {
                    continue;
                }
                
                // For file uploads, reject malformed names (unclosed brackets or double [[)
                // parse_key_parts returns empty segments for flattened names
                // If the original name had brackets but segments is empty, it was flattened due to malformation
                // Also reject if any segment starts with '[' (indicates double [[)
                let has_malformed_brackets = name.contains('[') && (
                    segments.is_empty() || 
                    segments.iter().any(|s| s.starts_with('['))
                );
                
                if has_malformed_brackets {
                    // Malformed name (unclosed brackets or double [[) - skip this file upload
                    continue;
                }
                
                let mut parts = vec![base.clone()];
                parts.append(&mut segments);
                
                if parts.len() > max_nesting_level {
                    // Exceeds max nesting - skip this file
                    continue;
                }
                
                if fname.is_empty() {
                    // Empty filename - error code 4 (UPLOAD_ERR_NO_FILE)
                    file_uploads.push((
                        name.clone(),
                        String::new(),
                        String::new(),
                        String::new(),
                        String::new(),
                        4, // UPLOAD_ERR_NO_FILE
                        0
                    ));
                } else {
                    // Extract basename from filename (for 'name' field)
                    // Full path is preserved in 'full_path' field
                    let basename = fname.rsplit(&['/', '\\'][..])
                        .next()
                        .unwrap_or(&fname)
                        .to_string();
                    
                    let file_size = content_trimmed.len();
                    
                    // Check MAX_FILE_SIZE (client-side hint) first
                    // Error code 2 = UPLOAD_ERR_FORM_SIZE
                    if let Some(max_size) = max_file_size {
                        if file_size > max_size {
                            file_uploads.push((
                                name.clone(),
                                basename.clone(),
                                fname.clone(),
                                String::new(),
                                String::new(),
                                2, // UPLOAD_ERR_FORM_SIZE
                                0
                            ));
                            continue;
                        }
                    }
                    
                    // Check upload_max_filesize (server-side limit)
                    // Error code 1 = UPLOAD_ERR_INI_SIZE
                    let upload_max_filesize = vm.context.config.ini_settings.get("upload_max_filesize")
                        .and_then(|v| self.parse_size_ini_value(v))
                        .unwrap_or(2 * 1024 * 1024); // Default 2MB
                    
                    if file_size > upload_max_filesize {
                        // File exceeds upload_max_filesize - error code 1 (UPLOAD_ERR_INI_SIZE)
                        file_uploads.push((
                            name.clone(),
                            basename.clone(),
                            fname.clone(),
                            String::new(),
                            String::new(),
                            1, // UPLOAD_ERR_INI_SIZE
                            0
                        ));
                        continue;
                    }
                    
                    // Create temporary file
                    use std::io::Write;
                    let tmp_dir = std::env::temp_dir();
                    let timestamp = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_nanos();
                    let tmp_name = format!("php{}_{}", std::process::id(), timestamp);
                    let tmp_path = tmp_dir.join(&tmp_name);
                    
                    let size = content_trimmed.len() as i64;
                    if let Ok(mut file) = std::fs::File::create(&tmp_path) {
                        let _ = file.write_all(content_trimmed.as_bytes());
                    }
                    
                    // Track uploaded file
                    let tmp_path_string = tmp_path.to_string_lossy().into_owned();
                    vm.context.uploaded_files.insert(tmp_path_string.clone());
                    
                    // Check if this is a partial upload (last part with no proper ending)
                    let error_code = if is_last_part && !has_proper_ending {
                        3 // UPLOAD_ERR_PARTIAL - file was only partially uploaded
                    } else {
                        0 // No error
                    };
                    
                    // For partial uploads, don't save the file
                    let (final_tmp_name, final_size) = if error_code == 3 {
                        (String::new(), 0)
                    } else {
                        (tmp_path_string, size)
                    };
                    
                    file_uploads.push((
                        name.clone(),
                        basename,        // name field: basename only
                        fname,           // full_path field: preserve directory structure
                        if error_code == 3 { String::new() } else { content_type.clone().unwrap_or_default() },
                        final_tmp_name,
                        error_code,
                        final_size
                    ));
                }
            } else {
                // Regular form field
                
                // Check if this is MAX_FILE_SIZE field
                if name == "MAX_FILE_SIZE" {
                    // Parse the value as file size limit
                    if let Ok(size) = content_trimmed.parse::<usize>() {
                        max_file_size = Some(size);
                    }
                }
                
                let value_handle = vm.arena.alloc(Val::String(Rc::new(content_trimmed.as_bytes().to_vec())));
                post_map.insert(ArrayKey::Str(Rc::new(name.into_bytes())), value_handle);
            }
        }
        
        // Set $_POST
        if !post_map.is_empty() {
            let post_array = vm.arena.alloc(Val::Array(Rc::new(ArrayData::from(post_map))));
            let post_sym = vm.context.interner.intern(b"_POST");
            vm.context.globals.insert(post_sym, post_array);
        }
        
        // Build inverted $_FILES structure from collected uploads
        // PHP inverts the structure: $_FILES['file']['name'][0] not $_FILES['file[0]']['name']
        // Group uploads by base field name while preserving order
        let mut files_builder: IndexMap<String, Vec<(Vec<String>, String, String, String, String, i64, i64)>> = IndexMap::new();
        
        // Group uploads by base field name
        for (field_name, name, full_path, mime_type, tmp_name, error, size) in file_uploads {
            let (base, segments) = Self::parse_key_parts(&field_name);
            if base.is_empty() {
                continue;
            }
            
            files_builder.entry(base.clone())
                .or_insert_with(Vec::new)
                .push((segments, name, full_path, mime_type, tmp_name, error, size));
        }
        
        // Build the final $_FILES structure
        let mut files_map = IndexMap::new();
        for (base_name, uploads) in files_builder {
            // Check if this is a scalar upload or array upload
            let is_array = uploads.iter().any(|(segments, _, _, _, _, _, _)| !segments.is_empty());
            
            if is_array {
                // Build inverted structure for array uploads
                let mut props = IndexMap::new();
                for prop_name in ["name", "full_path", "type", "tmp_name", "error", "size"] {
                    // Use ArrayData to handle proper index auto-increment
                    let mut prop_data = ArrayData::new();
                    for (segments, name, full_path, mime_type, tmp_name, error, size) in &uploads {
                        let value = match prop_name {
                            "name" => vm.arena.alloc(Val::String(Rc::new(name.clone().into_bytes()))),
                            "full_path" => vm.arena.alloc(Val::String(Rc::new(full_path.clone().into_bytes()))),
                            "type" => vm.arena.alloc(Val::String(Rc::new(mime_type.clone().into_bytes()))),
                            "tmp_name" => vm.arena.alloc(Val::String(Rc::new(tmp_name.clone().into_bytes()))),
                            "error" => vm.arena.alloc(Val::Int(*error)),
                            "size" => vm.arena.alloc(Val::Int(*size)),
                            _ => unreachable!()
                        };
                        if segments.is_empty() {
                            // This shouldn't happen in array mode, but handle it
                            prop_data.insert(ArrayKey::Int(0), value);
                        } else {
                            // Use first segment as key
                            let key = if segments[0].is_empty() {
                                // Empty bracket - use next auto index
                                ArrayKey::Int(prop_data.next_index())
                            } else if let Ok(idx) = segments[0].parse::<i64>() {
                                ArrayKey::Int(idx)
                            } else {
                                ArrayKey::Str(Rc::new(segments[0].as_bytes().to_vec()))
                            };
                            prop_data.insert(key, value);
                        }
                    }
                    props.insert(
                        ArrayKey::Str(Rc::new(prop_name.as_bytes().to_vec())),
                        vm.arena.alloc(Val::Array(Rc::new(prop_data)))
                    );
                }
                files_map.insert(
                    ArrayKey::Str(Rc::new(base_name.into_bytes())),
                    vm.arena.alloc(Val::Array(Rc::new(ArrayData::from(props))))
                );
            } else {
                // Scalar upload
                if let Some((_, name, full_path, mime_type, tmp_name, error, size)) = uploads.first() {
                    let mut props = IndexMap::new();
                    props.insert(ArrayKey::Str(Rc::new(b"name".to_vec())), 
                        vm.arena.alloc(Val::String(Rc::new(name.clone().into_bytes()))));
                    props.insert(ArrayKey::Str(Rc::new(b"full_path".to_vec())), 
                        vm.arena.alloc(Val::String(Rc::new(full_path.clone().into_bytes()))));
                    props.insert(ArrayKey::Str(Rc::new(b"type".to_vec())), 
                        vm.arena.alloc(Val::String(Rc::new(mime_type.clone().into_bytes()))));
                    props.insert(ArrayKey::Str(Rc::new(b"tmp_name".to_vec())), 
                        vm.arena.alloc(Val::String(Rc::new(tmp_name.clone().into_bytes()))));
                    props.insert(ArrayKey::Str(Rc::new(b"error".to_vec())), vm.arena.alloc(Val::Int(*error)));
                    props.insert(ArrayKey::Str(Rc::new(b"size".to_vec())), vm.arena.alloc(Val::Int(*size)));
                    
                    // Use numeric key for anonymous uploads (when base_name is a number)
                    let key = if let Ok(idx) = base_name.parse::<i64>() {
                        ArrayKey::Int(idx)
                    } else {
                        ArrayKey::Str(Rc::new(base_name.into_bytes()))
                    };
                    
                    files_map.insert(
                        key,
                        vm.arena.alloc(Val::Array(Rc::new(ArrayData::from(props))))
                    );
                }
            }
        }
        
        // Set $_FILES
        if !files_map.is_empty() {
            let files_array = vm.arena.alloc(Val::Array(Rc::new(ArrayData::from(files_map))));
            let files_sym = vm.context.interner.intern(b"_FILES");
            vm.context.globals.insert(files_sym, files_array);
        }
    }

    fn setup_server_superglobal(&self, vm: &mut VM, test: &PhptTest) -> Option<String> {
        use crate::core::value::{Val, ArrayData, ArrayKey};
        use indexmap::IndexMap;
        use std::rc::Rc;

        let mut server_map = IndexMap::new();
        let register_argc_argv_setting = vm
            .context
            .config
            .ini_settings
            .get("register_argc_argv")
            .cloned();
        let register_argc_argv = register_argc_argv_setting
            .as_deref()
            .map(|v| v != "0")
            .unwrap_or(false);
        let allow_get_derivation = register_argc_argv && register_argc_argv_setting.is_some();

        let mut argv_handle: Option<crate::core::value::Handle> = None;
        let mut argc_handle: Option<crate::core::value::Handle> = None;
        let mut warn_get_derivation = false;

        // Populate argv and argc from --ARGS-- section (CLI mode)
        // In CLI mode, these are always populated regardless of register_argc_argv
        if let Some(ref args_str) = test.sections.args {
            // Split args by whitespace
            let args: Vec<&str> = args_str.split_whitespace().collect();

            // Create argv array - first element is script name
            let mut argv_map = IndexMap::new();

            // argv[0] is the script name - use test file name with .php extension
            let script_basename = test.file_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("test");
            let script_name_str = format!("{}.php", script_basename);
            let script_name = Val::String(Rc::new(script_name_str.as_bytes().to_vec()));
            let script_handle = vm.arena.alloc(script_name);
            argv_map.insert(ArrayKey::Int(0), script_handle);

            // Add remaining arguments
            for (i, arg) in args.iter().enumerate() {
                let arg_val = Val::String(Rc::new(arg.as_bytes().to_vec()));
                let arg_handle = vm.arena.alloc(arg_val);
                argv_map.insert(ArrayKey::Int((i + 1) as i64), arg_handle);
            }

            let argv_array = Val::Array(Rc::new(ArrayData::from(argv_map)));
            let handle = vm.arena.alloc(argv_array);

            // Set argc (number of arguments including script name)
            let argc_val = Val::Int((args.len() + 1) as i64);
            let argc = vm.arena.alloc(argc_val);

            argv_handle = Some(handle);
            argc_handle = Some(argc);
        } else if allow_get_derivation && (test.sections.get.is_some() || test.sections.cgi) {
            // Handle argv/argc from GET query string only if register_argc_argv is enabled
            // This is controlled by allow_get_derivation which checks register_argc_argv
            let args: Vec<String> = if let Some(ref get_data) = test.sections.get {
                get_data.split('+')
                    .map(|s| String::from_utf8_lossy(&Self::url_decode(s)).to_string())
                    .collect()
            } else {
                Vec::new()
            };

            let mut argv_map = IndexMap::new();
            for (i, arg) in args.iter().enumerate() {
                let arg_val = Val::String(Rc::new(arg.as_bytes().to_vec()));
                let arg_handle = vm.arena.alloc(arg_val);
                argv_map.insert(ArrayKey::Int(i as i64), arg_handle);
            }

            let argv_array = Val::Array(Rc::new(ArrayData::from(argv_map)));
            let handle = vm.arena.alloc(argv_array);

            let argc_val = Val::Int(args.len() as i64);
            let argc = vm.arena.alloc(argc_val);

            argv_handle = Some(handle);
            argc_handle = Some(argc);
            warn_get_derivation = true;
        }

        // Add argc/argv to $_SERVER
        // When populated from ARGS (CLI), always add
        // When populated from GET, only add if register_argc_argv is enabled (controlled by allow_get_derivation)
        if let Some(argv) = argv_handle {
            server_map.insert(ArrayKey::Str(Rc::new(b"argv".to_vec())), argv);
        }
        if let Some(argc) = argc_handle {
            server_map.insert(ArrayKey::Str(Rc::new(b"argc".to_vec())), argc);
        }

        // Create $_SERVER array
        let server_array = Val::Array(Rc::new(ArrayData::from(server_map)));
        let server_handle = vm.arena.alloc(server_array);
        let server_sym = vm.context.interner.intern(b"_SERVER");
        vm.context.globals.insert(server_sym, server_handle);

        // Populate $argv/$argc variables when available
        if let (Some(argv), Some(argc)) = (argv_handle, argc_handle) {
            let argv_sym = vm.context.interner.intern(b"argv");
            let argc_sym = vm.context.interner.intern(b"argc");
            vm.context.globals.insert(argv_sym, argv);
            vm.context.globals.insert(argc_sym, argc);
        }

        if warn_get_derivation {
            Some(
                "Deprecated: Deriving $_SERVER['argv'] from the query string is deprecated. Configure register_argc_argv=0 to turn this message off in Unknown on line 1\n"
                    .to_string(),
            )
        } else {
            None
        }
    }

    /// Apply INI settings from --INI-- section to VM context
    fn apply_ini_settings(&self, vm: &mut VM, test: &PhptTest) {
        for (key, value) in &test.sections.ini {
            vm.context.config.ini_settings.insert(key.clone(), value.clone());
            
            // Apply special INI settings that need to be parsed
            if key == "error_reporting" {
                if let Some(level) = self.parse_error_reporting_level(value) {
                    vm.context.config.error_reporting = level;
                }
            }
        }
        
        // Apply memory_limit clamping based on max_memory_limit
        if let Some(max_memory_limit_str) = vm.context.config.ini_settings.get("max_memory_limit").cloned() {
            if let Some(max_memory_limit) = self.parse_size_ini_value(&max_memory_limit_str) {
                if let Some(memory_limit_str) = vm.context.config.ini_settings.get("memory_limit").cloned() {
                    // Parse memory_limit - handle -1 as unlimited
                    if memory_limit_str == "-1" {
                        // Silently clamp -1 to max_memory_limit (no warning for -1)
                        vm.context.config.ini_settings.insert(
                            "memory_limit".to_string(),
                            max_memory_limit_str.clone()
                        );
                    } else if let Some(memory_limit) = self.parse_size_ini_value(&memory_limit_str) {
                        if memory_limit > max_memory_limit {
                            // Emit warning when exceeding max
                            use crate::vm::engine::ErrorLevel;
                            let msg = format!(
                                "Failed to set memory_limit to {} bytes. Setting to max_memory_limit instead (currently: {} bytes)",
                                memory_limit, max_memory_limit
                            );
                            vm.trigger_error(ErrorLevel::Warning, &msg);
                            
                            // Clamp to max_memory_limit
                            vm.context.config.ini_settings.insert(
                                "memory_limit".to_string(),
                                max_memory_limit_str.clone()
                            );
                        }
                    }
                }
            }
        }
    }

    fn execute_source(&self, source: &str, vm: &mut VM, file_path: Option<&str>) -> Result<(), String> {
        let source_bytes = source.as_bytes();
        let arena = Bump::new();
        let lexer = Lexer::new(source_bytes);
        let mut parser = PhpParser::new(lexer, &arena);

        let program = parser.parse_program();

        if !program.errors.is_empty() {
            let mut error_msg = String::new();
            for error in program.errors {
                error_msg.push_str(&error.to_human_readable(source_bytes));
                error_msg.push('\n');
            }
            return Err(error_msg);
        }

        // Compile
        let mut emitter = Emitter::new(source_bytes, &mut vm.context.interner);
        if let Some(path) = file_path {
            emitter = emitter.with_file_path(path);
        }
        let (chunk, has_error) = emitter.compile(program.statements);

        if has_error {
            return Err("Compilation errors occurred".to_string());
        }

        // Execute
        if let Err(e) = vm.run(Rc::new(chunk)) {
            return Err(format!("Runtime error: {:?}", e));
        }

        // Flush output buffers
        crate::builtins::output_control::flush_all_output_buffers(vm)
            .map_err(|e| format!("Failed to flush output: {:?}", e))?;
        vm.flush_output()
            .map_err(|e| format!("Failed to flush output: {:?}", e))?;

        Ok(())
    }
    
    /// Parse error_reporting level from INI value
    /// Supports expressions like "E_ALL", "E_ALL & ~E_NOTICE", "E_ALL ^ E_WARNING", etc.
    fn parse_error_reporting_level(&self, value: &str) -> Option<u32> {
        // PHP error level constants
        const E_ERROR: u32 = 1;
        const E_WARNING: u32 = 2;
        const E_PARSE: u32 = 4;
        const E_NOTICE: u32 = 8;
        const E_CORE_ERROR: u32 = 16;
        const E_CORE_WARNING: u32 = 32;
        const E_COMPILE_ERROR: u32 = 64;
        const E_COMPILE_WARNING: u32 = 128;
        const E_USER_ERROR: u32 = 256;
        const E_USER_WARNING: u32 = 512;
        const E_USER_NOTICE: u32 = 1024;
        const E_STRICT: u32 = 2048;
        const E_RECOVERABLE_ERROR: u32 = 4096;
        const E_DEPRECATED: u32 = 8192;
        const E_USER_DEPRECATED: u32 = 16384;
        const E_ALL: u32 = 32767;
        
        // First try to parse as a numeric value
        if let Ok(level) = value.trim().parse::<u32>() {
            return Some(level);
        }
        
        // Parse simple expressions like "E_ALL ^ E_WARNING"
        // This is a simplified parser that handles common patterns
        let value = value.trim();
        
        // Replace constant names with values for simple evaluation
        let with_values = value
            .replace("E_ALL", &E_ALL.to_string())
            .replace("E_ERROR", &E_ERROR.to_string())
            .replace("E_WARNING", &E_WARNING.to_string())
            .replace("E_PARSE", &E_PARSE.to_string())
            .replace("E_NOTICE", &E_NOTICE.to_string())
            .replace("E_CORE_ERROR", &E_CORE_ERROR.to_string())
            .replace("E_CORE_WARNING", &E_CORE_WARNING.to_string())
            .replace("E_COMPILE_ERROR", &E_COMPILE_ERROR.to_string())
            .replace("E_COMPILE_WARNING", &E_COMPILE_WARNING.to_string())
            .replace("E_USER_ERROR", &E_USER_ERROR.to_string())
            .replace("E_USER_WARNING", &E_USER_WARNING.to_string())
            .replace("E_USER_NOTICE", &E_USER_NOTICE.to_string())
            .replace("E_STRICT", &E_STRICT.to_string())
            .replace("E_RECOVERABLE_ERROR", &E_RECOVERABLE_ERROR.to_string())
            .replace("E_DEPRECATED", &E_DEPRECATED.to_string())
            .replace("E_USER_DEPRECATED", &E_USER_DEPRECATED.to_string());
        
        // Simple expression evaluator for bitwise operations
        // Supports: &, |, ^, ~, parentheses
        self.evaluate_bitwise_expression(&with_values)
    }
    
    /// Simple bitwise expression evaluator
    fn evaluate_bitwise_expression(&self, expr: &str) -> Option<u32> {
        // For now, handle simple cases like "32767 ^ 2" (E_ALL ^ E_WARNING)
        let expr = expr.trim();
        
        // Handle XOR (^)
        if let Some(pos) = expr.find('^') {
            let left = self.evaluate_bitwise_expression(expr[..pos].trim())?;
            let right = self.evaluate_bitwise_expression(expr[pos+1..].trim())?;
            return Some(left ^ right);
        }
        
        // Handle AND (&)
        if let Some(pos) = expr.find('&') {
            // Check if it's &~ (AND NOT)
            let after_amp = expr[pos+1..].trim();
            if after_amp.starts_with('~') {
                let left = self.evaluate_bitwise_expression(expr[..pos].trim())?;
                let right = self.evaluate_bitwise_expression(after_amp[1..].trim())?;
                return Some(left & !right);
            } else {
                let left = self.evaluate_bitwise_expression(expr[..pos].trim())?;
                let right = self.evaluate_bitwise_expression(after_amp)?;
                return Some(left & right);
            }
        }
        
        // Handle OR (|)
        if let Some(pos) = expr.find('|') {
            let left = self.evaluate_bitwise_expression(expr[..pos].trim())?;
            let right = self.evaluate_bitwise_expression(expr[pos+1..].trim())?;
            return Some(left | right);
        }
        
        // Handle NOT (~)
        if expr.starts_with('~') {
            let value = self.evaluate_bitwise_expression(expr[1..].trim())?;
            return Some(!value);
        }
        
        // Parse as number
        expr.parse::<u32>().ok()
    }
    
    /// Parse INI size value (e.g., "1", "1K", "1M", "1G")
    /// Returns None if parsing fails, Some(0) for unlimited, Some(size) otherwise
    fn parse_size_ini_value(&self, value: &str) -> Option<usize> {
        let value = value.trim();
        if value.is_empty() {
            return None;
        }
        
        let (num_str, multiplier) = if value.ends_with('G') || value.ends_with('g') {
            (&value[..value.len()-1], 1024 * 1024 * 1024)
        } else if value.ends_with('M') || value.ends_with('m') {
            (&value[..value.len()-1], 1024 * 1024)
        } else if value.ends_with('K') || value.ends_with('k') {
            (&value[..value.len()-1], 1024)
        } else {
            (value, 1)
        };
        
        num_str.parse::<usize>().ok().map(|n| n * multiplier)
    }
    
    /// Parse post_max_size value - 0 means unlimited
    fn parse_post_max_size(&self, vm: &VM) -> Option<usize> {
        vm.context.config.ini_settings.get("post_max_size")
            .and_then(|v| self.parse_size_ini_value(v))
            .and_then(|size| if size == 0 { None } else { Some(size) })
    }
    
    /// Parse Content-Disposition parameter value with proper quote and escape handling
    /// Handles: unquoted, 'single-quoted', "double-quoted" with escape sequences
    fn parse_disposition_value(s: &str) -> String {
        let s = s.trim();
        if s.is_empty() {
            return String::new();
        }
        
        // Check for quotes
        let first_char = s.chars().next().unwrap();
        
        if first_char == '"' || first_char == '\'' {
            // Quoted value - parse with escape handling
            let quote_char = first_char;
            let mut result = String::new();
            let mut chars = s.chars().skip(1); // Skip opening quote
            
            while let Some(ch) = chars.next() {
                if ch == '\\' {
                    // Escape sequence
                    if let Some(next_ch) = chars.next() {
                        match next_ch {
                            '\\' => result.push('\\'),
                            '\'' if quote_char == '\'' => result.push('\''),
                            '"' if quote_char == '"' => result.push('"'),
                            // For other escapes within quotes, PHP keeps backslash+char
                            _ => {
                                result.push('\\');
                                result.push(next_ch);
                            }
                        }
                    } else {
                        result.push('\\');
                    }
                } else if ch == quote_char {
                    // Closing quote - done
                    break;
                } else {
                    result.push(ch);
                }
            }
            result
        } else {
            // Unquoted value - only \\ is an escape sequence
            let mut result = String::new();
            let mut chars = s.chars();
            
            while let Some(ch) = chars.next() {
                if ch == '\\' {
                    if let Some(next_ch) = chars.next() {
                        if next_ch == '\\' {
                            result.push('\\');
                        } else {
                            // Not an escape - keep both characters
                            result.push('\\');
                            result.push(next_ch);
                        }
                    } else {
                        result.push('\\');
                    }
                } else {
                    result.push(ch);
                }
            }
            result
        }
    }
}

impl Default for PhptExecutor {
    fn default() -> Self {
        Self::new().expect("Failed to create PhptExecutor")
    }
}
