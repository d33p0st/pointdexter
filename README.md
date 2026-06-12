# pointdexter

A lock-free hierarchical key/value tree with C FFI, written in Rust.

## Overview

`pointdexter` provides a concurrent, lock-free tree of named **points** — nodes that each hold a multi-valued key/value store. Points can be attached to form parent/child hierarchies, and the entire structure supports both scoped and global searches across the tree.

The library ships as both an `rlib` (for Rust consumers) and a `cdylib` (for C, C++, and Python via FFI).

## Features

- **Lock-free** — built on `crossbeam-skiplist` and `dashmap` for high-concurrency workloads
- **Hierarchical** — attach/detach points to build arbitrary trees; cycle detection is built in
- **Multi-value keys** — a single key can hold multiple values per point
- **Scoped & global search** — search within a subtree or across all live points
- **C FFI** — a stable C API (`pointdexter.h`) for use from C, C++, Python, and any language with a C FFI bridge
- **Null-safe FFI** — every C function handles null pointers gracefully

## Installation

### Rust

Add to your `Cargo.toml`:

```toml
[dependencies]
pointdexter = "0.1"
```

### C / C++

Build the dynamic library:

```sh
cargo build --release
```

The compiled library will be at `target/release/libpointdexter.so` (Linux) or `target/release/libpointdexter.dylib` (macOS). Include `pointdexter.h` or `pointdexter.hpp` from this repository.

### Python

A Python binding is provided in `pointdexter.py`. It wraps the C FFI layer via `ctypes`.

## Usage

### Rust

```rust
use pointdexter::prelude::*;

// Create a point
let node = Point::new("server");

// Insert key/value pairs
node.insert("host", "localhost");
node.insert("host", "127.0.0.1"); // multi-value

// Retrieve values
let first = node.get_first("host"); // Some("localhost")
let all   = node.get_values("host"); // ["localhost", "127.0.0.1"]

// Build a hierarchy
let child = Point::new("worker");
node.attach(&child).unwrap();

// Search the subtree rooted at `node`
let results = node.search("host");
```

### C

```c
#include "pointdexter.h"

PdPoint *server = pd_point("server");
pd_insert(server, "host", "localhost");

const char *val = pd_get_first(server, "host");
printf("%s\n", val);
pd_string_free((char *)val);

pd_point_free(server);
```

## API Reference

See [`pointdexter.h`](pointdexter.h) for the full C API and [`pointdexter.hpp`](pointdexter.hpp) for the C++ wrapper. Rust API docs can be generated with:

```sh
cargo doc --open
```

## Building

```sh
# Debug build
cargo build

# Optimised release build
cargo build --release

# Security-hardened build (DWARF + overflow checks, for sanitiser runs)
cargo build --profile hardened
```

## Running Tests

```sh
cargo test
```

## License

MIT — see [LICENSE](LICENSE).

## Author

Soumyo Deep Gupta — [deep.main.ac@gmail.com](mailto:deep.main.ac@gmail.com)
