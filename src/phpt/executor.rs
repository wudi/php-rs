use crate::compiler::emitter::Emitter;
use crate::parser::lexer::Lexer;
use crate::parser::parser::Parser as PhpParser;
use crate::phpt::matcher::{match_output, ExpectationType};
use crate::phpt::output_writer::BufferedOutputWriter;
use crate::phpt::parser::PhptTest;
use crate::runtime::context::{EngineBuilder, EngineContext};
use crate::vm::engine::VM;
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

        // TODO: Apply INI settings (when INI handling is available)
        // For now, we'll skip INI settings

        // TODO: Set up $argv and $argc from ARGS section
        // For now, we'll skip command-line arguments

        // Set up HTTP superglobals from test sections
        self.setup_superglobals(&mut vm, test);

        let output_writer = BufferedOutputWriter::new();
        vm.set_output_writer(Box::new(output_writer.clone()));

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
        use crate::core::value::{Val, ArrayData, Handle};
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
        use crate::core::value::{Val, ArrayData, ArrayKey};
        use indexmap::IndexMap;
        use std::rc::Rc;

        let mut map = IndexMap::new();

        for pair in query.split('&') {
            if let Some(pos) = pair.find('=') {
                let key = &pair[..pos];
                let value = &pair[pos + 1..];

                // URL decode
                let decoded_key = Self::url_decode(key);
                let decoded_value = Self::url_decode(value);

                // Create key
                let array_key = ArrayKey::Str(Rc::new(decoded_key.into_bytes()));

                // Create value and allocate
                let val = Val::String(Rc::new(decoded_value.into_bytes()));
                let val_handle = vm.arena.alloc(val);

                map.insert(array_key, val_handle);
            }
        }

        let array_data = ArrayData::from(map);
        let array_val = Val::Array(Rc::new(array_data));
        vm.arena.alloc(array_val)
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
