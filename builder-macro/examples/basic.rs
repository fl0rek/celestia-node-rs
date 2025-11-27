use builder_macro::Builder;
use std::{collections::HashMap, io::Bytes, str::FromStr, time::Duration};
use tonic::transport::{Endpoint, Uri};

/// Example struct with both required and optional fields
#[derive(Builder, Debug, Clone, PartialEq)]
struct ServerConfig {
    /// Required: server hostname
    #[builder(into)]
    pub host: String,
    /// Required: server port
    #[builder(default = 80)]
    pub port: u16,
    /// Optional: connection timeout
    pub timeout: Option<Duration>,
    /// Optional: enable TLS
    pub tls: Option<bool>,
    #[builder(skip, default)]
    pub metadata: HashMap<String, String>,
}

impl ServerConfigBuilder {
    /// Hand-written method that sets multiple fields for localhost
    pub fn localhost(self, port: u16) -> Self {
        self.host("127.0.0.1".to_string()).port(port)
    }

    /// Hand-written method for production configuration
    pub fn production(self, host: impl Into<String>) -> Self {
        self.host(host)
            .port(443)
            .tls(true)
            .timeout(Duration::from_secs(60))
    }

    pub fn metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata
            .get_or_insert_default()
            .insert(key.into(), value.into());
        self
    }
}

#[derive(Builder)]
struct TransportBuilder<T = BoxedTransport> {
    pub transport: T,
}

struct BoxedTransport;

impl BoxedTransport {}

impl<T> TransportBuilder<T> {
    fn url(mut self, url: &str) -> anyhow::Result<TransportBuilder<Endpoint>> {
        let channel = Endpoint::from_str(url)?;
        //.tls_config(tls_config)?
        //.connect_lazy();
        Ok(self.with_transport(channel))
    }

    fn with_transport<T1>(mut self, transport: T1) -> TransportBuilder<T1> {
        TransportBuilder { transport }
    }
}

fn main() {
    println!("=== Builder Macro Example ===\n");

    // Example 1: Basic usage with all fields
    println!("1. Basic usage with all fields:");
    let config1 = ServerConfig::builder()
        .host("example.com")
        .port(8080)
        .timeout(Duration::from_secs(30))
        .tls(true)
        .build()
        .expect("Failed to build config");
    assert_eq!(
        dbg!(config1),
        ServerConfig {
            host: "example.com".to_string(),
            port: 8080,
            timeout: Some(Duration::from_secs(30)),
            tls: Some(true),
            metadata: HashMap::default(),
        }
    );

    // Example 2: Only required fields
    println!("2. Only required fields:");
    let config2 = ServerConfig::builder()
        .host("api.example.com".to_string())
        .build()
        .expect("Failed to build config");
    assert_eq!(
        dbg!(config2),
        ServerConfig {
            host: "api.example.com".to_string(),
            port: 80,
            timeout: None,
            tls: None,
            metadata: HashMap::default(),
        }
    );

    // Example 3: Using hand-written localhost method
    println!("3. Using hand-written localhost() method:");
    let config3 = ServerConfig::builder()
        .localhost(3000)
        .build()
        .expect("Failed to build config");
    assert_eq!(
        dbg!(config3),
        ServerConfig {
            host: "127.0.0.1".to_string(),
            port: 3000,
            timeout: None,
            tls: None,
            metadata: HashMap::default(),
        }
    );

    // Example 4: Using hand-written production method
    println!("4. Using hand-written production() method:");
    let config4 = ServerConfig::builder()
        .production("prod.example.com".to_string())
        .build()
        .expect("Failed to build config");
    assert_eq!(
        dbg!(config4),
        ServerConfig {
            host: "prod.example.com".to_string(),
            port: 443,
            timeout: Some(Duration::from_secs(60)),
            tls: Some(true),
            metadata: HashMap::default(),
        }
    );

    // Example 5: Using hand-written development method
    println!("5. Using hand-written development() method:");
    let config5 = ServerConfig::builder()
        .production("lumina.eiger.co")
        .metadata("auth-token", "foo")
        .metadata("version", "1.0")
        .build()
        .expect("Failed to build config");
    assert_eq!(
        dbg!(config5),
        ServerConfig {
            host: "lumina.eiger.co".to_string(),
            port: 443,
            timeout: Some(Duration::from_secs(60)),
            tls: Some(true),
            metadata: HashMap::from_iter([
                ("auth-token".to_string(), "foo".to_string()),
                ("version".to_string(), "1.0".to_string())
            ])
        }
    );

    // Example 6: Runtime validation - missing required field
    println!("6. Runtime validation - missing required field:");
    let result = ServerConfig::builder()
        .port(9000)
        // Missing host! (host is required, port has a default)
        .build();

    assert_eq!(result, Err("Required field 'host' not set".to_string()));
    println!("Error (as expected): {}\n", result.unwrap_err());

    // Example 7: Chaining methods
    println!("7. Chaining multiple methods:");
    let config7 = ServerConfig::builder()
        .host("staging.example.com".to_string())
        .port(8443)
        .timeout(Duration::from_secs(45))
        .tls(true)
        .build()
        .expect("Failed to build config");
    assert_eq!(
        dbg!(config7),
        ServerConfig {
            host: "staging.example.com".to_string(),
            port: 8443,
            timeout: Some(Duration::from_secs(45)),
            tls: Some(true),
            metadata: HashMap::default(),
        }
    );

    // Example 8: Overriding hand-written method values
    println!("8. Overriding hand-written method values:");
    let config8 = ServerConfig::builder()
        .localhost(8080)
        .timeout(Duration::from_secs(120)) // Override timeout from localhost()
        .tls(true) // Add TLS
        .build()
        .expect("Failed to build config");
    assert_eq!(
        dbg!(config8),
        ServerConfig {
            host: "127.0.0.1".to_string(),
            port: 8080,
            timeout: Some(Duration::from_secs(120)),
            tls: Some(true),
            metadata: HashMap::default(),
        }
    );

    println!("=== All examples completed successfully! ===");
}
