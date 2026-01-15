//! PHP-FPM: FastCGI Process Manager (multi-threaded, async).
//!
//! Uses tokio with LocalSet to support !Send VM state (Rc pointers).
//! Each worker thread runs its own single-threaded tokio runtime.

use bumpalo::Bump;
use clap::Parser;
use php_rs::compiler::emitter::Emitter;
use php_rs::fcgi;
use php_rs::parser::lexer::Lexer;
use php_rs::parser::parser::Parser as PhpParser;
use php_rs::runtime::context::EngineContext;
use php_rs::sapi::fpm::FpmRequest;
use php_rs::vm::engine::{OutputWriter, VM, VmError};
use std::cell::RefCell;
use std::io::Write;
use std::net::TcpListener as StdTcpListener;
use std::os::unix::net::UnixListener as StdUnixListener;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Global metrics for the FPM server.
struct FpmMetrics {
    start_time: SystemTime,
    accepted_conn: AtomicU64,
    active_requests: AtomicU32,
}
use std::thread;
use std::time::SystemTime;
use tokio::net::{TcpListener, TcpStream, UnixListener, UnixStream};
use tokio::task::LocalSet;

#[derive(Parser)]
#[command(name = "php-fpm")]
#[command(about = "PHP FastCGI Process Manager (async/threaded)", long_about = None)]
struct Cli {
    /// Listen on TCP (e.g., "127.0.0.1:9000")
    #[arg(short = 'b', long, conflicts_with = "socket")]
    bind: Option<String>,

    /// Listen on Unix socket (e.g., "/tmp/php-fpm.sock")
    #[arg(short = 's', long, conflicts_with = "bind")]
    socket: Option<PathBuf>,

    /// Number of worker threads
    #[arg(short = 'w', long, default_value = "4")]
    workers: usize,
}

static SHUTDOWN: AtomicBool = AtomicBool::new(false);

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Install signal handler
    ctrlc::set_handler(|| {
        eprintln!("[php-fpm] Received shutdown signal");
        SHUTDOWN.store(true, Ordering::Relaxed);
    })?;

    eprintln!("[php-fpm] Starting {} workers", cli.workers);

    let metrics = Arc::new(FpmMetrics {
        start_time: SystemTime::now(),
        accepted_conn: AtomicU64::new(0),
        active_requests: AtomicU32::new(0),
    });

    if let Some(bind_addr) = cli.bind {
        eprintln!("[php-fpm] Listening on TCP {}", bind_addr);
        let listener = StdTcpListener::bind(&bind_addr)?;
        listener.set_nonblocking(true)?;
        run_workers(cli.workers, ListenerSource::Tcp(listener), metrics)?;
    } else if let Some(socket_path) = cli.socket {
        eprintln!(
            "[php-fpm] Listening on Unix socket {}",
            socket_path.display()
        );
        // Remove existing socket
        let _ = std::fs::remove_file(&socket_path);
        let listener = StdUnixListener::bind(&socket_path)?;
        listener.set_nonblocking(true)?;
        run_workers(cli.workers, ListenerSource::Unix(listener), metrics)?;
    } else {
        eprintln!("[php-fpm] Error: must specify --bind or --socket");
        std::process::exit(1);
    }

    Ok(())
}

enum ListenerSource {
    Tcp(StdTcpListener),
    Unix(StdUnixListener),
}

fn run_workers(
    workers: usize,
    source: ListenerSource,
    metrics: Arc<FpmMetrics>,
) -> anyhow::Result<()> {
    let mut handles = Vec::new();

    for id in 0..workers {
        let source_clone = match &source {
            ListenerSource::Tcp(l) => ListenerSource::Tcp(l.try_clone()?),
            ListenerSource::Unix(l) => ListenerSource::Unix(l.try_clone()?),
        };
        let metrics = metrics.clone();

        let handle = thread::Builder::new()
            .stack_size(32 * 1024 * 1024)
            .spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            let local = LocalSet::new();

            local.block_on(&rt, async move {
                let context = php_rs::runtime::context::EngineBuilder::new()
                    .with_core_extensions()
                    .build()
                    .expect("Failed to build engine");
                eprintln!("[php-fpm] Worker {} started", id);

                match source_clone {
                    ListenerSource::Tcp(l) => {
                        let listener = TcpListener::from_std(l).unwrap();
                        loop {
                            if SHUTDOWN.load(Ordering::Relaxed) {
                                break;
                            }

                            if let Ok((stream, _)) = listener.accept().await {
                                let engine = context.clone();
                                let metrics = metrics.clone();
                                tokio::task::spawn_local(async move {
                                    if let Err(e) =
                                        handle_fastcgi_connection(stream, engine, metrics).await
                                    {
                                        eprintln!("[php-fpm] Connection error: {}", e);
                                    }
                                });
                            }
                        }
                    }
                    ListenerSource::Unix(l) => {
                        let listener = UnixListener::from_std(l).unwrap();
                        loop {
                            if SHUTDOWN.load(Ordering::Relaxed) {
                                break;
                            }
                            if let Ok((stream, _)) = listener.accept().await {
                                let engine = context.clone();
                                let metrics = metrics.clone();
                                tokio::task::spawn_local(async move {
                                    if let Err(e) =
                                        handle_fastcgi_unix_connection(stream, engine, metrics).await
                                    {
                                        eprintln!("[php-fpm] Connection error: {}", e);
                                    }
                                });
                            }
                        }
                    }
                }
                eprintln!("[php-fpm] Worker {} stopping", id);
            });
        })
        .expect("Failed to spawn php-fpm worker thread");
        handles.push(handle);
    }

    for handle in handles {
        let _ = handle.join();
    }

    Ok(())
}

/// Handle a FastCGI connection (may have multiple requests if keep-alive).
async fn handle_fastcgi_connection(
    stream: TcpStream,
    engine: Arc<EngineContext>,
    metrics: Arc<FpmMetrics>,
) -> Result<(), anyhow::Error> {
    // Convert to std stream since our fcgi module uses std::io
    let std_stream = stream.into_std()?;
    std_stream.set_nonblocking(false)?; // Ensure blocking mode

    handle_fastcgi_connection_sync(std_stream, engine, metrics).await
}

/// Handle Unix stream connection.
async fn handle_fastcgi_unix_connection(
    stream: UnixStream,
    engine: Arc<EngineContext>,
    metrics: Arc<FpmMetrics>,
) -> Result<(), anyhow::Error> {
    let std_stream = stream.into_std()?;
    std_stream.set_nonblocking(false)?;

    handle_fastcgi_connection_sync(std_stream, engine, metrics).await
}

/// Synchronous FastCGI connection handler.
async fn handle_fastcgi_connection_sync<S>(
    stream: S,
    engine: Arc<EngineContext>,
    metrics: Arc<FpmMetrics>,
) -> Result<(), anyhow::Error>
where
    S: std::io::Read + std::io::Write + 'static,
{
    // Increment accepted connections
    metrics.accepted_conn.fetch_add(1, Ordering::Relaxed);

    // Use RefCell to allow multiple borrows
    let stream = Rc::new(std::cell::RefCell::new(stream));

    loop {
        // Read one complete FastCGI request or management record
        let result = {
            let mut stream_ref = stream.borrow_mut();
            match fcgi::request::read_request(&mut *stream_ref) {
                Ok(res) => res,
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    break;
                }
                Err(e) => {
                    eprintln!("[php-fpm] FastCGI protocol error: {}", e);
                    return Err(e.into());
                }
            }
        };

        let request = match result {
            fcgi::request::ResultRequest::Request(req) => req,
            fcgi::request::ResultRequest::Management {
                record_type,
                content,
            } => {
                if record_type == fcgi::RecordType::GetValues {
                    handle_get_values(&mut *stream.borrow_mut(), &content)?;
                }
                continue;
            }
        };

        let request_id = request.request_id;
        let keep_conn = request.keep_conn;

        // Capture request start time for REQUEST_TIME/REQUEST_TIME_FLOAT
        let request_time = SystemTime::now();

        // Convert to FpmRequest (SAPI adapter)
        let fpm_req = match FpmRequest::from_fcgi(&request, request_time) {
            Ok(req) => req,
            Err(e) => {
                eprintln!("[php-fpm] Request parse error: {}", e);
                // Send error response
                let mut stream_ref = stream.borrow_mut();
                let body = format!("Status: 500 Error\r\n\r\n{}", e);
                fcgi::protocol::write_record(
                    &mut *stream_ref,
                    fcgi::RecordType::Stdout,
                    request_id,
                    body.as_bytes(),
                )?;
                fcgi::protocol::write_record(
                    &mut *stream_ref,
                    fcgi::RecordType::Stdout,
                    request_id,
                    &[],
                )?;

                let end_body = fcgi::protocol::EndRequestBody {
                    app_status: 500,
                    protocol_status: fcgi::ProtocolStatus::RequestComplete,
                };
                fcgi::protocol::write_record(
                    &mut *stream_ref,
                    fcgi::RecordType::EndRequest,
                    request_id,
                    &end_body.encode(),
                )?;
                stream_ref.flush()?;

                if !keep_conn {
                    break;
                }
                continue;
            }
        };

        // Execute PHP script (handles sending response)
        execute_php(&engine, &fpm_req, stream.clone(), request_id, metrics.clone()).await;

        // Handle keep-alive
        if !keep_conn {
            break;
        }
    }

    Ok(())
}

/// Handle /status page request.
fn handle_status_page<W: Write>(stream: Rc<RefCell<W>>, request_id: u16, metrics: Arc<FpmMetrics>) {
    let mut response = Vec::new();
    let now = SystemTime::now();
    let uptime = now
        .duration_since(metrics.start_time)
        .unwrap_or_default()
        .as_secs();

    let accepted = metrics.accepted_conn.load(Ordering::Relaxed);
    let active = metrics.active_requests.load(Ordering::Relaxed);

    let start_since = metrics
        .start_time
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    writeln!(response, "pool:                 www").unwrap();
    writeln!(response, "process manager:      static").unwrap();
    writeln!(response, "start time:           {}", start_since).unwrap();
    writeln!(response, "start since:          {}", uptime).unwrap();
    writeln!(response, "accepted conn:        {}", accepted).unwrap();
    writeln!(response, "listen queue:         0").unwrap();
    writeln!(response, "max listen queue:     0").unwrap();
    writeln!(response, "listen queue len:     0").unwrap();
    writeln!(response, "idle processes:       0").unwrap(); // We don't track idle process vs active in this simple model yet
    writeln!(response, "active processes:     {}", active).unwrap();
    writeln!(response, "total processes:      0").unwrap();
    writeln!(response, "max active processes: {}", active).unwrap();
    writeln!(response, "max children reached: 0").unwrap();
    writeln!(response, "slow requests:        0").unwrap();

    let mut stream_ref = stream.borrow_mut();
    let headers = format!(
        "Status: 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n",
        response.len()
    );
    let _ = fcgi::protocol::write_record(
        &mut *stream_ref,
        fcgi::RecordType::Stdout,
        request_id,
        headers.as_bytes(),
    );
    let _ = fcgi::protocol::write_record(
        &mut *stream_ref,
        fcgi::RecordType::Stdout,
        request_id,
        &response,
    );
    let _ = fcgi::protocol::write_record(&mut *stream_ref, fcgi::RecordType::Stdout, request_id, &[]);

    let end_body = fcgi::protocol::EndRequestBody {
        app_status: 0,
        protocol_status: fcgi::ProtocolStatus::RequestComplete,
    };
    let _ = fcgi::protocol::write_record(
        &mut *stream_ref,
        fcgi::RecordType::EndRequest,
        request_id,
        &end_body.encode(),
    );
    let _ = stream_ref.flush();
}

/// Handle /ping page request.
fn handle_ping_page<W: Write>(stream: Rc<RefCell<W>>, request_id: u16) {
    let response = b"pong";
    let mut stream_ref = stream.borrow_mut();
    let headers = format!(
        "Status: 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n",
        response.len()
    );
    let _ = fcgi::protocol::write_record(
        &mut *stream_ref,
        fcgi::RecordType::Stdout,
        request_id,
        headers.as_bytes(),
    );
    let _ = fcgi::protocol::write_record(
        &mut *stream_ref,
        fcgi::RecordType::Stdout,
        request_id,
        response,
    );
    let _ = fcgi::protocol::write_record(&mut *stream_ref, fcgi::RecordType::Stdout, request_id, &[]);

    let end_body = fcgi::protocol::EndRequestBody {
        app_status: 0,
        protocol_status: fcgi::ProtocolStatus::RequestComplete,
    };
    let _ = fcgi::protocol::write_record(
        &mut *stream_ref,
        fcgi::RecordType::EndRequest,
        request_id,
        &end_body.encode(),
    );
    let _ = stream_ref.flush();
}

/// Handle FCGI_GET_VALUES management record.
fn handle_get_values<W: Write>(writer: &mut W, content: &[u8]) -> std::io::Result<()> {
    let requested = match fcgi::protocol::decode_params(content) {
        Ok(params) => params,
        Err(e) => {
            eprintln!("[php-fpm] GetValues decode error: {}", e);
            return Ok(());
        }
    };

    let mut response = Vec::new();
    for (name, _) in requested {
        match name.as_slice() {
            b"FCGI_MAX_CONNS" => {
                response.push((b"FCGI_MAX_CONNS".to_vec(), b"100".to_vec()));
            }
            b"FCGI_MAX_REQS" => {
                response.push((b"FCGI_MAX_REQS".to_vec(), b"1000".to_vec()));
            }
            b"FCGI_MPXS_CONNS" => {
                // We currently don't support multiplexing on a single connection.
                response.push((b"FCGI_MPXS_CONNS".to_vec(), b"0".to_vec()));
            }
            _ => {
                // Ignore unknown values
            }
        }
    }

    let encoded = fcgi::protocol::encode_params(&response);
    fcgi::protocol::write_record(writer, fcgi::RecordType::GetValuesResult, 0, &encoded)?;
    writer.flush()?;

    Ok(())
}

/// Execute PHP script. Handles sending response and finishing request.
async fn execute_php<W: Write + 'static>(
    engine: &Arc<EngineContext>,
    fpm_req: &FpmRequest,
    stream: Rc<RefCell<W>>,
    request_id: u16,
    metrics: Arc<FpmMetrics>,
) {
    let request_uri = fpm_req
        .server_vars
        .get(b"REQUEST_URI".as_slice())
        .map(|v| String::from_utf8_lossy(v));

    if let Some(uri) = request_uri {
        if uri == "/status" {
            handle_status_page(stream, request_id, metrics);
            return;
        } else if uri == "/ping" {
            handle_ping_page(stream, request_id);
            return;
        }
    }

    let source = match tokio::fs::read(&fpm_req.script_filename).await {
        Ok(s) => s,
        Err(e) => {
            // Internal error (script not found/readable)
            let error = format!("Error reading script: {}", e);
            let mut stream_ref = stream.borrow_mut();
            let body = format!(
                "Status: 500 Internal Server Error\r\nContent-Type: text/plain\r\n\r\n{}",
                error
            );
            let _ = fcgi::protocol::write_record(
                &mut *stream_ref,
                fcgi::RecordType::Stdout,
                request_id,
                body.as_bytes(),
            );
            let _ = fcgi::protocol::write_record(
                &mut *stream_ref,
                fcgi::RecordType::Stdout,
                request_id,
                &[],
            );
            let end_body = fcgi::protocol::EndRequestBody {
                app_status: 500,
                protocol_status: fcgi::ProtocolStatus::RequestComplete,
            };
            let _ = fcgi::protocol::write_record(
                &mut *stream_ref,
                fcgi::RecordType::EndRequest,
                request_id,
                &end_body.encode(),
            );
            let _ = stream_ref.flush();
            return;
        }
    };

    let arena = Bump::new();
    let lexer = Lexer::new(&source);
    let mut parser = PhpParser::new(lexer, &arena);
    let program = parser.parse_program();

    let error_buffer = Arc::new(Mutex::new(Vec::new()));
    let mut vm = VM::new_with_sapi(Arc::clone(engine), php_rs::sapi::SapiMode::FpmFcgi);

    // Use SAPI to initialize superglobals
    php_rs::sapi::init_superglobals(
        &mut vm,
        php_rs::sapi::SapiMode::FpmFcgi,
        fpm_req.server_vars.clone(),
        fpm_req.env_vars.clone(),
        fpm_req.get_vars.clone(),
        fpm_req.post_vars.clone(),
        fpm_req.cookie_vars.clone(),
        fpm_req.files_vars.clone(),
    );

    let emitter = Emitter::new(&source, &mut vm.context.interner)
        .with_file_path(fpm_req.script_filename.clone());
    let (bytecode, _) = emitter.compile(&program.statements);

    // Setup FPM Output Writer
    let writer = FpmOutputWriter::new(stream.clone(), request_id);
    let writer_rc = Rc::new(RefCell::new(writer));
    let wrapper = FpmOutputWriterWrapper {
        inner: writer_rc.clone(),
    };

    vm.set_output_writer(Box::new(wrapper));
    vm.set_error_handler(Box::new(BufferedErrorHandler::new(error_buffer.clone())));

    // Run VM
    let _ = vm.run(Rc::new(bytecode));
    if let Err(err) = php_rs::builtins::output_control::flush_all_output_buffers(&mut vm) {
        let mut buffer = error_buffer.lock().unwrap();
        buffer.extend_from_slice(format!("Warning: {}\n", err).as_bytes());
    }

    // Finish request if not already done
    let errors = error_buffer.lock().unwrap().clone();
    let mut w = writer_rc.borrow_mut();

    if !w.finished {
        // Normal completion (script didn't call fastcgi_finish_request)
        // Send headers
        let _ = w.send_headers(
            &vm.context.headers,
            vm.context.http_status.unwrap_or(200) as u16,
        );

        // Flush buffer (body)
        let _ = w.flush_buffer();

        // Send errors to client via STDERR
        if !errors.is_empty() {
            let mut stream_ref = w.stream.borrow_mut();
            const MAX_CHUNK: usize = 65535;
            for chunk in errors.chunks(MAX_CHUNK) {
                let _ = fcgi::protocol::write_record(
                    &mut *stream_ref,
                    fcgi::RecordType::Stderr,
                    request_id,
                    chunk,
                );
            }
            let _ = fcgi::protocol::write_record(
                &mut *stream_ref,
                fcgi::RecordType::Stderr,
                request_id,
                &[],
            );
        }

        // Finish
        let _ = w.finish();
    } else {
        // Script called fastcgi_finish_request, connection closed.
        // Log errors to stderr console instead of trying to send to client.
        if !errors.is_empty() {
            eprintln!(
                "[php-fpm] [Script Errors after finish]: {}",
                String::from_utf8_lossy(&errors)
            );
        }
    }
}

/// Output writer that writes to FastCGI stream.
struct FpmOutputWriter<W: Write> {
    stream: Rc<RefCell<W>>,
    request_id: u16,
    buffer: Vec<u8>,
    headers_sent: bool,
    finished: bool,
}

impl<W: Write> FpmOutputWriter<W> {
    fn new(stream: Rc<RefCell<W>>, request_id: u16) -> Self {
        Self {
            stream,
            request_id,
            buffer: Vec::new(),
            headers_sent: false,
            finished: false,
        }
    }

    fn write_to_buffer(&mut self, bytes: &[u8]) {
        self.buffer.extend_from_slice(bytes);
    }

    fn flush_buffer(&mut self) -> Result<(), std::io::Error> {
        if self.buffer.is_empty() {
            return Ok(());
        }

        let mut stream = self.stream.borrow_mut();
        const MAX_CHUNK: usize = 65535;
        for chunk in self.buffer.chunks(MAX_CHUNK) {
            fcgi::protocol::write_record(
                &mut *stream,
                fcgi::RecordType::Stdout,
                self.request_id,
                chunk,
            )?;
        }
        self.buffer.clear();
        Ok(())
    }

    fn send_headers(
        &mut self,
        headers: &[php_rs::runtime::context::HeaderEntry],
        status: u16,
    ) -> Result<(), std::io::Error> {
        if self.headers_sent {
            return Ok(());
        }

        let mut response = Vec::new();
        // Status line
        let reason = http_reason_phrase(status);
        write!(response, "Status: {} {}\r\n", status, reason)?;

        // Headers
        let mut has_content_type = false;
        for header in headers {
            response.extend_from_slice(&header.line);
            response.extend_from_slice(b"\r\n");
            if let Some(ref key) = header.key {
                if key.eq_ignore_ascii_case(b"content-type") {
                    has_content_type = true;
                }
            }
        }

        // Default Content-Type if not set
        if !has_content_type {
            write!(response, "Content-Type: text/html; charset=UTF-8\r\n")?;
        }

        // End of headers
        write!(response, "\r\n")?;

        let mut stream = self.stream.borrow_mut();
        fcgi::protocol::write_record(
            &mut *stream,
            fcgi::RecordType::Stdout,
            self.request_id,
            &response,
        )?;

        self.headers_sent = true;
        Ok(())
    }

    fn finish(&mut self) -> Result<(), std::io::Error> {
        if self.finished {
            return Ok(());
        }

        // Flush any remaining buffer
        self.flush_buffer()?;

        let mut stream = self.stream.borrow_mut();

        // Empty STDOUT to signal end
        fcgi::protocol::write_record(&mut *stream, fcgi::RecordType::Stdout, self.request_id, &[])?;

        // END_REQUEST
        let end_body = fcgi::protocol::EndRequestBody {
            app_status: 0,
            protocol_status: fcgi::ProtocolStatus::RequestComplete,
        };
        fcgi::protocol::write_record(
            &mut *stream,
            fcgi::RecordType::EndRequest,
            self.request_id,
            &end_body.encode(),
        )?;

        stream.flush()?;
        self.finished = true;
        Ok(())
    }
}

struct FpmOutputWriterWrapper<W: Write> {
    inner: Rc<RefCell<FpmOutputWriter<W>>>,
}

impl<W: Write> OutputWriter for FpmOutputWriterWrapper<W> {
    fn write(&mut self, bytes: &[u8]) -> Result<(), VmError> {
        let mut inner = self.inner.borrow_mut();
        if !inner.finished {
            inner.write_to_buffer(bytes);
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<(), VmError> {
        let mut inner = self.inner.borrow_mut();
        // Only flush if headers are sent AND not finished.
        if inner.headers_sent && !inner.finished {
            inner
                .flush_buffer()
                .map_err(|e: std::io::Error| VmError::RuntimeError(e.to_string()))?;
        }
        Ok(())
    }

    fn send_headers(
        &mut self,
        headers: &[php_rs::runtime::context::HeaderEntry],
        status: u16,
    ) -> Result<(), VmError> {
        let mut inner = self.inner.borrow_mut();
        inner
            .send_headers(headers, status)
            .map_err(|e: std::io::Error| VmError::RuntimeError(e.to_string()))
    }

    fn finish(&mut self) -> Result<(), VmError> {
        let mut inner = self.inner.borrow_mut();
        inner
            .finish()
            .map_err(|e: std::io::Error| VmError::RuntimeError(e.to_string()))
    }
}

/// Error handler that captures errors/warnings to a buffer.
struct BufferedErrorHandler {
    buffer: Arc<Mutex<Vec<u8>>>,
}

impl BufferedErrorHandler {
    fn new(buffer: Arc<Mutex<Vec<u8>>>) -> Self {
        Self { buffer }
    }
}

impl php_rs::vm::engine::ErrorHandler for BufferedErrorHandler {
    fn report(&mut self, level: php_rs::vm::engine::ErrorLevel, message: &str) {
        use php_rs::vm::engine::ErrorLevel;
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
        let formatted = format!(
            "PHP {}: {}
",
            level_str, message
        );
        self.buffer
            .lock()
            .unwrap()
            .extend_from_slice(formatted.as_bytes());
    }
}

/// Get HTTP reason phrase for status code.
fn http_reason_phrase(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        301 => "Moved Permanently",
        302 => "Found",
        304 => "Not Modified",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        500 => "Internal Server Error",
        501 => "Not Implemented",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        _ => "Unknown",
    }
}
