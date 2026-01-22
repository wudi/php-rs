use bumpalo::Bump;
use clap::Parser;
use indexmap::IndexMap;
use php_rs::compiler::emitter::Emitter;
use php_rs::core::value::{ArrayData, ArrayKey, Val};
use php_rs::parser::lexer::Lexer;
use php_rs::parser::parser::Parser as PhpParser;
use php_rs::runtime::context::{EngineBuilder, EngineContext};
use php_rs::vm::engine::{OutputWriter, StdoutWriter, VM, VmError};
use rustyline::DefaultEditor;
use rustyline::error::ReadlineError;
use std::cell::RefCell;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "php")]
#[command(about = "PHP Interpreter in Rust", long_about = None)]
struct Cli {
    /// Run interactively
    #[arg(short = 'a', long)]
    interactive: bool,

    /// Script file to run
    #[arg(name = "FILE")]
    file: Option<PathBuf>,

    /// Arguments to pass to the script
    #[arg(allow_hyphen_values = true, trailing_var_arg = true)]
    args: Vec<String>,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    if cli.interactive {
        run_repl()?;
    } else if let Some(file) = cli.file {
        run_file(file, cli.args)?;
    } else {
        // If no arguments, show help
        use clap::CommandFactory;
        Cli::command().print_help()?;
    }

    Ok(())
}

fn create_engine() -> anyhow::Result<Arc<EngineContext>> {
    let builder = EngineBuilder::new();

    builder
        .with_core_extensions()
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to build engine: {}", e))
}

#[derive(Default)]
struct ReplOutputState {
    wrote_output: bool,
    last_byte: Option<u8>,
}

impl ReplOutputState {
    fn reset(&mut self) {
        self.wrote_output = false;
        self.last_byte = None;
    }

    fn note_write(&mut self, bytes: &[u8]) {
        if let Some(&last) = bytes.last() {
            self.wrote_output = true;
            self.last_byte = Some(last);
        }
    }

    fn needs_trailing_newline(&self) -> bool {
        self.wrote_output && self.last_byte != Some(b'\n')
    }
}

struct TrackingOutputWriter<W: OutputWriter> {
    inner: W,
    state: Rc<RefCell<ReplOutputState>>,
}

impl<W: OutputWriter> TrackingOutputWriter<W> {
    fn new(inner: W, state: Rc<RefCell<ReplOutputState>>) -> Self {
        Self { inner, state }
    }
}

impl<W: OutputWriter> OutputWriter for TrackingOutputWriter<W> {
    fn write(&mut self, bytes: &[u8]) -> Result<(), VmError> {
        self.state.borrow_mut().note_write(bytes);
        self.inner.write(bytes)
    }

    fn flush(&mut self) -> Result<(), VmError> {
        self.inner.flush()
    }
}

fn append_repl_trailing_newline_if_needed(
    vm: &mut VM,
    output_state: &Rc<RefCell<ReplOutputState>>,
) -> Result<(), VmError> {
    if output_state.borrow().needs_trailing_newline() {
        vm.print_bytes(b"\n").map_err(VmError::RuntimeError)?;
        vm.flush_output()?;
    }
    Ok(())
}

/// Decode readline history format (converts \040 to space, \134 to backslash, etc.)
fn decode_readline_history_line(line: &str) -> String {
    let mut result = String::new();
    let mut chars = line.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\\' {
            // Support legacy "\\" escapes written by older php-rs versions.
            if let Some(&next_ch) = chars.peek() {
                if next_ch == '\\' {
                    result.push('\\');
                    chars.next();
                    continue;
                }
            }
            // Check for octal escape sequence (\nnn)
            let next_chars: String = chars.clone().take(3).collect();
            if next_chars.len() == 3 && next_chars.chars().all(|c| c.is_ascii_digit()) {
                if let Ok(code) = u8::from_str_radix(&next_chars, 8) {
                    result.push(code as char);
                    chars.next();
                    chars.next();
                    chars.next();
                    continue;
                }
            }
            // If not a valid escape, keep the backslash
            result.push(ch);
        } else {
            result.push(ch);
        }
    }
    result
}

/// Encode string to readline history format (converts space to \040, etc.)
fn encode_readline_history_line(line: &str) -> String {
    let mut result = String::new();
    for ch in line.chars() {
        match ch {
            ' ' => result.push_str("\\040"),
            '\t' => result.push_str("\\011"),
            '\n' => result.push_str("\\012"),
            '\r' => result.push_str("\\015"),
            '\\' => result.push_str("\\134"),
            _ if ch.is_ascii() && !ch.is_control() => result.push(ch),
            _ => {
                // For non-ASCII or control chars, use octal encoding
                let code = ch as u32;
                if code <= 255 {
                    result.push_str(&format!("\\{:03o}", code));
                } else {
                    result.push(ch); // Keep as-is for Unicode
                }
            }
        }
    }
    result
}

/// Load readline history file (compatible with PHP's .php_history format)
fn load_readline_history(path: &Path, editor: &mut DefaultEditor) -> anyhow::Result<()> {
    if !path.exists() {
        return Ok(());
    }
    
    let file = fs::File::open(path)?;
    let reader = BufReader::new(file);
    
    for line in reader.lines() {
        let line = line?;
        // Skip the _HiStOrY_V2_ header
        if line == "_HiStOrY_V2_" {
            continue;
        }
        let decoded = decode_readline_history_line(&line);
        if !decoded.is_empty() {
            let _ = editor.add_history_entry(&decoded);
        }
    }
    
    Ok(())
}

/// Save readline history file (compatible with PHP's .php_history format)
fn save_readline_history(path: &Path, editor: &DefaultEditor) -> anyhow::Result<()> {
    let mut file = fs::File::create(path)?;
    
    // Write the readline v2 header
    writeln!(file, "_HiStOrY_V2_")?;
    
    // Write history entries
    let history = editor.history();
    for entry in history.iter() {
        let encoded = encode_readline_history_line(entry);
        writeln!(file, "{}", encoded)?;
    }
    
    Ok(())
}

fn run_repl() -> anyhow::Result<()> {
    let mut rl = DefaultEditor::new()?;
    
    // Use ~/.php_history for REPL history (compatible with PHP's readline format)
    let history_path = std::env::var("HOME")
        .map(|home| PathBuf::from(home).join(".php_history"))
        .unwrap_or_else(|_| PathBuf::from(".php_history"));
    
    // Load history in readline format
    if let Err(e) = load_readline_history(&history_path, &mut rl) {
        eprintln!("Warning: Could not load history: {}", e);
    }

    println!("Interactive shell");
    println!("Type 'exit' or 'quit' to quit");

    let engine_context = create_engine()?;
    let mut vm = VM::new_with_sapi(engine_context, php_rs::sapi::SapiMode::Cli);
    let output_state = Rc::new(RefCell::new(ReplOutputState::default()));
    let output_writer = TrackingOutputWriter::new(StdoutWriter::default(), output_state.clone());
    vm.set_output_writer(Box::new(output_writer));

    loop {
        let readline = rl.readline("php > ");
        match readline {
            Ok(line) => {
                let line: &str = line.trim();
                if line == "exit" || line == "quit" {
                    break;
                }
                rl.add_history_entry(line)?;

                // Execute line
                // In REPL, we might want to wrap in <?php ?> if not present?
                // Native PHP -a expects code without <?php ?> usually?
                // Actually php -a (interactive shell) expects PHP code.
                // If I type `echo "hello";` it works.
                // If I type `<?php echo "hello";` it might also work or fail depending on implementation.
                // Let's assume raw PHP code.
                // But the parser might expect `<?php` tag?
                // Let's check `parser` behavior.

                let source_code = if line.starts_with("<?php") {
                    line.to_string()
                } else {
                    format!("<?php {}", line)
                };

                output_state.borrow_mut().reset();
                if let Err(e) = execute_source(&source_code, None, &mut vm) {
                    println!("Error: {:?}", e);
                    continue;
                }
                if let Err(e) = append_repl_trailing_newline_if_needed(&mut vm, &output_state) {
                    println!("Error: {:?}", e);
                }
            }
            Err(ReadlineError::Interrupted) => {
                break;
            }
            Err(ReadlineError::Eof) => {
                println!("CTRL-D");
                break;
            }
            Err(err) => {
                println!("Error: {:?}", err);
                break;
            }
        }
    }
    
    // Save history in readline format
    if let Err(e) = save_readline_history(&history_path, &rl) {
        eprintln!("Warning: Could not save history: {}", e);
    }
    
    Ok(())
}

fn run_file(path: PathBuf, args: Vec<String>) -> anyhow::Result<()> {
    let source = fs::read_to_string(&path)?;
    let script_name = path.to_string_lossy().into_owned();
    let canonical_path = path.canonicalize().unwrap_or_else(|_| path.clone());

    // Change working directory to script directory
    if let Some(parent) = canonical_path.parent() {
        std::env::set_current_dir(parent)?;
    }

    let engine_context = create_engine()?;
    let mut vm = VM::new_with_sapi(engine_context, php_rs::sapi::SapiMode::Cli);

    // Fix $_SERVER variables to match the script being run
    let server_sym = vm.context.interner.intern(b"_SERVER");
    if let Some(server_handle) = vm.context.globals.get(&server_sym).copied() {
        // 1. Get the array data Rc
        let mut array_data_rc = if let Val::Array(rc) = &vm.arena.get(server_handle).value {
            rc.clone()
        } else {
            Rc::new(ArrayData::new())
        };

        // 2. Prepare values to insert (allocating in arena)
        // SCRIPT_FILENAME
        let script_filename = canonical_path.to_string_lossy().into_owned();
        let val_handle_filename = vm
            .arena
            .alloc(Val::String(Rc::new(script_filename.into_bytes())));

        // SCRIPT_NAME
        let script_name_str = path.to_string_lossy().into_owned();
        let val_handle_script_name = vm
            .arena
            .alloc(Val::String(Rc::new(script_name_str.clone().into_bytes())));

        // PHP_SELF
        let val_handle_php_self = vm
            .arena
            .alloc(Val::String(Rc::new(script_name_str.into_bytes())));

        // DOCUMENT_ROOT - set to script directory for CLI
        let doc_root = canonical_path
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        let val_handle_doc_root = vm.arena.alloc(Val::String(Rc::new(doc_root.into_bytes())));

        // PWD - current working directory
        let pwd = std::env::current_dir()
            .ok()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        let val_handle_pwd = vm.arena.alloc(Val::String(Rc::new(pwd.into_bytes())));

        // 3. Modify the array data
        let array_data = Rc::make_mut(&mut array_data_rc);

        array_data.insert(
            ArrayKey::Str(Rc::new(b"SCRIPT_FILENAME".to_vec())),
            val_handle_filename,
        );
        array_data.insert(
            ArrayKey::Str(Rc::new(b"SCRIPT_NAME".to_vec())),
            val_handle_script_name,
        );
        array_data.insert(
            ArrayKey::Str(Rc::new(b"PHP_SELF".to_vec())),
            val_handle_php_self,
        );
        array_data.insert(
            ArrayKey::Str(Rc::new(b"DOCUMENT_ROOT".to_vec())),
            val_handle_doc_root,
        );
        array_data.insert(ArrayKey::Str(Rc::new(b"PWD".to_vec())), val_handle_pwd);

        // 4. Update the global variable with the new Rc
        let slot = vm.arena.get_mut(server_handle);
        slot.value = Val::Array(array_data_rc);
    }

    // Populate $argv and $argc
    let mut argv_map = IndexMap::new();

    // argv[0] is the script name
    argv_map.insert(
        ArrayKey::Int(0),
        vm.arena
            .alloc(Val::String(Rc::new(script_name.into_bytes()))),
    );

    // Remaining args
    for (i, arg) in args.iter().enumerate() {
        argv_map.insert(
            ArrayKey::Int((i + 1) as i64),
            vm.arena
                .alloc(Val::String(Rc::new(arg.clone().into_bytes()))),
        );
    }

    let argv_handle = vm.arena.alloc(Val::Array(ArrayData::from(argv_map).into()));
    let argc_handle = vm.arena.alloc(Val::Int((args.len() + 1) as i64));

    let argv_symbol = vm.context.interner.intern(b"argv");
    let argc_symbol = vm.context.interner.intern(b"argc");

    vm.context.globals.insert(argv_symbol, argv_handle);
    vm.context.globals.insert(argc_symbol, argc_handle);

    execute_source(&source, Some(&canonical_path), &mut vm)
        .map_err(|e| anyhow::anyhow!("VM Error: {:?}", e))?;

    Ok(())
}

fn execute_source(source: &str, file_path: Option<&Path>, vm: &mut VM) -> Result<(), VmError> {
    let source_bytes = source.as_bytes();
    let arena = Bump::new();
    let lexer = Lexer::new(source_bytes);
    let mut parser = PhpParser::new(lexer, &arena);

    let program = parser.parse_program();

    if !program.errors.is_empty() {
        for error in program.errors {
            println!("{}", error.to_human_readable(source_bytes));
        }
        return Ok(());
    }

    // Compile
    let mut emitter = Emitter::new(source_bytes, &mut vm.context.interner);
    if let Some(path) = file_path {
        let path_string = path.to_string_lossy().into_owned();
        emitter = emitter.with_file_path(path_string);
    }
    let (chunk, _has_error) = emitter.compile(program.statements);

    // Run
    if let Err(err) = vm.run(Rc::new(chunk)) {
        if let Some((file, line)) = vm.current_location() {
            eprintln!("Runtime error in {} on line {}: {:?}", file, line, err);
        }
        vm.reset_after_error();
        return Err(err);
    }

    php_rs::builtins::output_control::flush_all_output_buffers(vm)
        .map_err(VmError::RuntimeError)?;
    vm.flush_output()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct BufferedState {
        pending: Vec<u8>,
        flushed: Vec<u8>,
    }

    struct BufferedOutputWriter {
        state: Arc<Mutex<BufferedState>>,
    }

    impl BufferedOutputWriter {
        fn new(state: Arc<Mutex<BufferedState>>) -> Self {
            Self { state }
        }
    }

    impl OutputWriter for BufferedOutputWriter {
        fn write(&mut self, bytes: &[u8]) -> Result<(), VmError> {
            let mut state = self.state.lock().unwrap();
            state.pending.extend_from_slice(bytes);
            Ok(())
        }

        fn flush(&mut self) -> Result<(), VmError> {
            let mut state = self.state.lock().unwrap();
            let pending = std::mem::take(&mut state.pending);
            state.flushed.extend_from_slice(&pending);
            Ok(())
        }
    }

    #[test]
    fn repl_appends_newline_when_output_has_no_trailing_newline() {
        let engine_context = create_engine().expect("engine context");
        let mut vm = VM::new_with_sapi(engine_context, php_rs::sapi::SapiMode::Cli);

        let output_state = Rc::new(RefCell::new(ReplOutputState::default()));
        let state = Arc::new(Mutex::new(BufferedState::default()));
        let writer = TrackingOutputWriter::new(
            BufferedOutputWriter::new(state.clone()),
            output_state.clone(),
        );
        vm.set_output_writer(Box::new(writer));

        output_state.borrow_mut().reset();
        execute_source("<?php echo 123;", None, &mut vm).expect("execute source");
        append_repl_trailing_newline_if_needed(&mut vm, &output_state)
            .expect("append trailing newline");

        let output = state.lock().unwrap().flushed.clone();
        assert_eq!(output, b"123\n");
    }
}
