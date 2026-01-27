use crate::runtime::context::HeaderEntry;
use crate::vm::engine::{ErrorHandler, ErrorLevel, OutputWriter, VmError};
use std::sync::{Arc, Mutex};

#[derive(Default, Clone)]
pub struct BufferedState {
    pub output: Vec<u8>,
}

#[derive(Clone)]
pub struct BufferedOutputWriter {
    pub(crate) state: Arc<Mutex<BufferedState>>,
}

impl BufferedOutputWriter {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(BufferedState::default())),
        }
    }

    pub fn get_output(&self) -> String {
        let state = self.state.lock().unwrap();
        String::from_utf8_lossy(&state.output).to_string()
    }
}

impl Default for BufferedOutputWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl OutputWriter for BufferedOutputWriter {
    fn write(&mut self, data: &[u8]) -> Result<(), VmError> {
        let mut state = self.state.lock().unwrap();
        state.output.extend_from_slice(data);
        Ok(())
    }

    fn flush(&mut self) -> Result<(), VmError> {
        Ok(())
    }

    fn send_headers(&mut self, _headers: &[HeaderEntry], _status: u16) -> Result<(), VmError> {
        Ok(())
    }

    fn finish(&mut self) -> Result<(), VmError> {
        Ok(())
    }
}

/// Error handler that writes to the same buffer as output
#[derive(Clone)]
pub struct BufferedErrorHandler {
    state: Arc<Mutex<BufferedState>>,
}

impl BufferedErrorHandler {
    pub fn new(state: Arc<Mutex<BufferedState>>) -> Self {
        Self { state }
    }
}

impl ErrorHandler for BufferedErrorHandler {
    fn report(&mut self, level: ErrorLevel, message: &str) {
        let level_str = match level {
            ErrorLevel::Notice => "Notice",
            ErrorLevel::Warning => "Warning",
            ErrorLevel::Error => "Error",
            ErrorLevel::ParseError => "Parse error",
            ErrorLevel::UserNotice => "User notice",
            ErrorLevel::UserWarning => "User warning",
            ErrorLevel::UserError => "User error",
            ErrorLevel::Deprecated => "Deprecated",
        };
        let formatted = format!("{}: {}\n", level_str, message);
        let mut state = self.state.lock().unwrap();
        state.output.extend_from_slice(formatted.as_bytes());
    }
}
