//! Regression test for php-fpm worker stack usage with deep expressions.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::process::{Child, Command};
use std::thread;
use std::time::Duration;

struct FpmServer {
    child: Child,
    socket_path: String,
}

impl FpmServer {
    fn start(socket_path: &str) -> Self {
        let binary = env!("CARGO_BIN_EXE_php-fpm");

        let _ = std::fs::remove_file(socket_path);

        let child = Command::new(binary)
            .arg("--socket")
            .arg(socket_path)
            .arg("--workers")
            .arg("1")
            .spawn()
            .expect("Failed to start php-fpm");

        for _ in 0..50 {
            if std::path::Path::new(socket_path).exists() {
                thread::sleep(Duration::from_millis(100));
                return Self {
                    child,
                    socket_path: socket_path.to_string(),
                };
            }
            thread::sleep(Duration::from_millis(100));
        }

        panic!("php-fpm socket not created within timeout");
    }
}

impl Drop for FpmServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

fn encode_name_value(name: &[u8], value: &[u8]) -> Vec<u8> {
    let mut result = Vec::new();
    let name_len = name.len();
    if name_len < 128 {
        result.push(name_len as u8);
    } else {
        let len = (name_len as u32) | 0x8000_0000;
        result.extend_from_slice(&len.to_be_bytes());
    }

    let value_len = value.len();
    if value_len < 128 {
        result.push(value_len as u8);
    } else {
        let len = (value_len as u32) | 0x8000_0000;
        result.extend_from_slice(&len.to_be_bytes());
    }

    result.extend_from_slice(name);
    result.extend_from_slice(value);
    result
}

fn make_record(record_type: u8, request_id: u16, content: &[u8]) -> Vec<u8> {
    let version = 1u8;
    let content_length = content.len() as u16;
    let padding_length = ((8 - (content.len() % 8)) % 8) as u8;

    let mut record = Vec::new();
    record.push(version);
    record.push(record_type);
    record.extend_from_slice(&request_id.to_be_bytes());
    record.extend_from_slice(&content_length.to_be_bytes());
    record.push(padding_length);
    record.push(0);
    record.extend_from_slice(content);
    record.extend_from_slice(&vec![0; padding_length as usize]);

    record
}

fn send_fcgi_request(socket_path: &str, script_path: &str) -> String {
    let mut stream = UnixStream::connect(socket_path).expect("Failed to connect");
    let request_id = 1u16;

    let begin_body = vec![0, 1, 0, 0, 0, 0, 0, 0];
    stream
        .write_all(&make_record(1, request_id, &begin_body))
        .unwrap();

    let mut params = Vec::new();
    params.extend_from_slice(&encode_name_value(
        b"SCRIPT_FILENAME",
        script_path.as_bytes(),
    ));
    params.extend_from_slice(&encode_name_value(b"REQUEST_METHOD", b"GET"));
    params.extend_from_slice(&encode_name_value(b"QUERY_STRING", b""));
    params.extend_from_slice(&encode_name_value(b"SERVER_PROTOCOL", b"HTTP/1.1"));
    params.extend_from_slice(&encode_name_value(b"REQUEST_URI", b"/deep.php"));
    params.extend_from_slice(&encode_name_value(b"HTTP_HOST", b"localhost"));
    stream
        .write_all(&make_record(4, request_id, &params))
        .unwrap();
    stream.write_all(&make_record(4, request_id, &[])).unwrap();

    stream.write_all(&make_record(5, request_id, &[])).unwrap();

    let mut response = Vec::new();
    stream.read_to_end(&mut response).unwrap();

    let mut stdout_data = Vec::new();
    let mut pos = 0;
    while pos + 8 <= response.len() {
        let rec_type = response[pos + 1];
        let content_len = u16::from_be_bytes([response[pos + 4], response[pos + 5]]) as usize;
        let padding_len = response[pos + 6] as usize;

        pos += 8;
        if rec_type == 6 && content_len > 0 {
            stdout_data.extend_from_slice(&response[pos..pos + content_len]);
        }
        pos += content_len + padding_len;
    }

    String::from_utf8_lossy(&stdout_data).to_string()
}

#[test]
fn test_fpm_deep_expression_does_not_overflow() {
    let socket = "/tmp/test-fpm-deep.sock";
    let _server = FpmServer::start(socket);

    let script_path = std::env::temp_dir().join("test_deep.php");
    let depth = 200usize;
    let mut expr = String::with_capacity(depth * 2 + 2);
    for _ in 0..depth {
        expr.push('(');
    }
    expr.push('1');
    for _ in 0..depth {
        expr.push(')');
    }
    let script = format!("<?php echo {}; ", expr);
    std::fs::write(&script_path, script).unwrap();

    let response = send_fcgi_request(socket, script_path.to_str().unwrap());

    assert!(
        response.contains("Status: 200"),
        "expected response headers, got: {}",
        response
    );
    assert!(
        response.contains('1'),
        "expected script output, got: {}",
        response
    );
}
