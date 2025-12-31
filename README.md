# php-rs

A PHP interpreter written in Rust. This project is currently in an experimental state.

## Features

- Core PHP language support
- Standard library extensions (BCMath, JSON, MySQLi, PDO, OpenSSL, Zip, Zlib, etc.)
- CLI interface (`php`)
- FastCGI Process Manager (`php-fpm`)

## Getting Started

### Prerequisites

- Rust (latest stable release)

### Building

Clone the repository and build using Cargo:

```bash
git clone https://github.com/wudi/php-rs.git
cd php-rs
cargo build --release
```

### Usage

#### CLI

Run a PHP script:

```bash
cargo run --bin php -- script.php
```

Interactive shell:

```bash
cargo run --bin php
```

#### FPM

Start the PHP-FPM server:

```bash
cargo run --bin php-fpm
```

## Testing

Run the test suite:

```bash
cargo test
```

## License

This project is licensed under the MIT License.

Created by AI
