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

        if let Err(e) = self.execute_source(skipif_code, &mut vm) {
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

        // Apply INI settings from --INI-- section
        self.apply_ini_settings(&mut vm, test);

        // Set up HTTP superglobals from test sections
        self.setup_superglobals(&mut vm, test);

        // Set up $_SERVER superglobal with argv/argc
        let argv_warning = self.setup_server_superglobal(&mut vm, test);

        let output_writer = BufferedOutputWriter::new();
        vm.set_output_writer(Box::new(output_writer.clone()));

        if let Some(warning) = argv_warning {
            let mut writer = output_writer.clone();
            // Best-effort: prepend warning so EXPECTF patterns match PHP behavior.
            let _ = writer.write(warning.as_bytes());
        }

        if let Err(e) = self.execute_source(&test.sections.file, &mut vm) {
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

        self.execute_source(clean_code, &mut vm)
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
            let post_array = self.parse_query_string(post_data, vm);
            let post_sym = vm.context.interner.intern(b"_POST");
            vm.context.globals.insert(post_sym, post_array);
        }

        // Parse and set $_COOKIE
        if let Some(ref cookie_data) = test.sections.cookie {
            let cookie_array = self.parse_query_string(cookie_data, vm);
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

            self.insert_query_pair(&mut result, &decoded_key, &decoded_value, vm);
        }

        let array_data = ArrayData::from(result);
        let array_val = Val::Array(Rc::new(array_data));
        vm.arena.alloc(array_val)
    }

    fn insert_query_pair(
        &self,
        result: &mut indexmap::IndexMap<crate::core::value::ArrayKey, crate::core::value::Handle>,
        key: &str,
        value: &str,
        vm: &mut VM,
    ) {
        let (base, mut segments) = Self::parse_key_parts(key);
        if base.is_empty() {
            return;
        }

        let mut parts = Vec::with_capacity(1 + segments.len());
        parts.push(base);
        parts.append(&mut segments);

        self.insert_parts(result, &parts, value, vm);
    }

    fn parse_key_parts(key: &str) -> (String, Vec<String>) {
        let mut base = String::new();
        let mut segments = Vec::new();
        let mut chars = key.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '[' {
                let mut segment = String::new();
                while let Some(&inner) = chars.peek() {
                    chars.next();
                    if inner == ']' {
                        break;
                    }
                    segment.push(inner);
                }
                segments.push(segment);
            } else if ch == ']' {
                // Ignore stray closing brackets to mimic PHP's tolerant parsing
                continue;
            } else {
                base.push(ch);
            }
        }

        (base, segments)
    }

    fn insert_parts(
        &self,
        map: &mut indexmap::IndexMap<crate::core::value::ArrayKey, crate::core::value::Handle>,
        parts: &[String],
        value: &str,
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
            let val = Val::String(Rc::new(value.as_bytes().to_vec()));
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

    fn url_decode(s: &str) -> String {
        let mut result = String::new();
        let mut chars = s.chars().peekable();

        while let Some(ch) = chars.next() {
            match ch {
                '+' => result.push(' '),
                '%' => {
                    let hex: String = chars.by_ref().take(2).collect();
                    if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                        result.push(byte as char);
                    } else {
                        result.push('%');
                        result.push_str(&hex);
                    }
                }
                _ => result.push(ch),
            }
        }

        result
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
        if let Some(ref args_str) = test.sections.args {
            // Split args by whitespace
            let args: Vec<&str> = args_str.split_whitespace().collect();

            // Create argv array - first element is script name
            let mut argv_map = IndexMap::new();

            // argv[0] is the script name
            let script_name = Val::String(Rc::new(b"test.php".to_vec()));
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
            // Handle argv/argc from GET query string (for tests like 011.phpt)
            let args: Vec<String> = if let Some(ref get_data) = test.sections.get {
                get_data.split('+').map(Self::url_decode).collect()
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
        }
    }

    fn execute_source(&self, source: &str, vm: &mut VM) -> Result<(), String> {
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
        let emitter = Emitter::new(source_bytes, &mut vm.context.interner);
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
}

impl Default for PhptExecutor {
    fn default() -> Self {
        Self::new().expect("Failed to create PhptExecutor")
    }
}
