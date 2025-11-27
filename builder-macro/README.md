# Builder Macro

A derive macro for creating builder patterns with runtime validation, inspired by the `bon` crate but with runtime checking instead of compile-time type states.

## Features

- **Automatic setter generation** for struct fields
- **Runtime validation** of required fields
- **Hand-written methods** can be added alongside generated ones
- **Into conversion** - use `#[builder(into)]` to accept `impl Into<T>`
- **Default values** - use `#[builder(default = expr)]` for optional fields
- **Optional fields** - `Option<T>` fields are automatically optional
- **Simple API** - just derive `Builder` on your struct

## Usage

### Basic Example

```rust
use builder_macro::Builder;

#[derive(Builder)]
struct ServerConfig {
    host: String,
    port: u16,
    timeout: Option<Duration>,
}

// Usage
let config = ServerConfig::builder()
    .host("example.com".to_string())
    .port(8080)
    .timeout(Duration::from_secs(30))
    .build()
    .unwrap();
```

### With Hand-Written Methods

You can add your own methods to the builder alongside the generated setters:

```rust
use builder_macro::Builder;

#[derive(Builder)]
struct ServerConfig {
    host: String,
    port: u16,
    timeout: Option<Duration>,
}

impl ServerConfigBuilder {
    // Hand-written method that sets multiple fields
    pub fn localhost(self, port: u16) -> Self {
        self.host("127.0.0.1".to_string()).port(port)
    }

    pub fn production(self) -> Self {
        self.host("prod.example.com".to_string())
            .port(443)
            .timeout(Duration::from_secs(60))
    }
}

// Usage
let config = ServerConfig::builder()
    .localhost(8080)
    .build()
    .unwrap();
```

### Into Conversion

Use `#[builder(into)]` to make the setter accept `impl Into<T>` instead of `T`. This is useful for types like `String` where you want to accept `&str`:

```rust
#[derive(Builder)]
struct ServerConfig {
    #[builder(into)]
    host: String,
    port: u16,
}

// Usage - can pass &str instead of String
let config = ServerConfig::builder()
    .host("example.com")  // Automatically converts &str to String
    .port(8080)
    .build()
    .unwrap();
```

### Default Values

Use `#[builder(default = expr)]` to provide a default value for a field. Fields with defaults are no longer required:

```rust
#[derive(Builder)]
struct ServerConfig {
    host: String,
    #[builder(default = 80)]
    port: u16,
    timeout: Option<Duration>,
}

// Usage - port is optional now
let config = ServerConfig::builder()
    .host("example.com".to_string())
    .build()
    .unwrap();
// config.port will be 80
```

### Skipping Fields

Use `#[builder(skip)]` to prevent setter generation for a field:

```rust
#[derive(Builder)]
struct Config {
    url: String,
    #[builder(skip)]
    internal_state: InternalState,
}

impl ConfigBuilder {
    // You can set the skipped field manually
    pub fn with_state(mut self, state: InternalState) -> Self {
        self.internal_state = Some(state);
        self
    }
}
```

## How It Works

1. The macro generates a `{YourStruct}Builder` struct with all fields wrapped in `Option`
2. Setter methods are generated for each field (unless marked with `#[builder(skip)]`)
3. A `builder()` constructor is added to your original struct
4. A `build()` method validates that all required fields are set and constructs your struct
5. Required fields are validated at runtime - the build method returns `Result<YourStruct, String>`
6. Fields with `Option<T>` type or `#[builder(default = expr)]` are not required
7. For `Option<T>` fields, two methods are generated:
   - `field(value: T)` - wraps the value in Some
   - `maybe_field(value: Option<T>)` - sets the Option directly

## Differences from `bon`

- **Runtime checking** instead of compile-time type states
- **Simpler** - no complex type-level machinery
- **More flexible** - easier to add hand-written methods
- **Different trade-offs** - catches missing fields at runtime rather than compile time

## Comparison with `grpc-macros`

This macro is inspired by the patterns in `grpc-macros` but serves a different purpose:
- `grpc-macros` generates gRPC method implementations from signatures
- `builder-macro` generates builder patterns from struct definitions

Both use similar attribute-based APIs and code generation techniques with `syn` and `quote`.
