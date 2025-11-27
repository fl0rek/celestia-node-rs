//! Compatibility layer for exporting gRPC functionality via uniffi

use std::ops::DerefMut;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use k256::ecdsa::VerifyingKey;
use uniffi::Object;

mod grpc_client;

use crate::GrpcClient as RustGrpcClient;
use crate::GrpcClientBuilder as RustBuilder;
use crate::builder::build_transport;
use crate::client::{AccountState, grpc_client_builder};
use crate::error::MetadataError;
use crate::grpc::Context;
use crate::signer::{BoxedDocSigner, UniffiSigner, UniffiSignerBox};

pub use grpc_client::GrpcClient;

pub type Result<T, E = GrpcClientBuilderError> = std::result::Result<T, E>;

/// Errors returned when building Grpc Client
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum GrpcClientBuilderError {
    /// Builder was already used to create a client
    #[error("Builder already consumed")]
    BuilderConsumed,

    /// Error creating transport
    #[error("error creating transport: {msg}")]
    TonicTransportError {
        /// error message
        msg: String,
    },

    /// Invalid account public key
    #[error("invalid account public key")]
    InvalidAccountPublicKey,

    /// Invalid account private key
    #[error("invalid account private key")]
    InvalidAccountPrivateKey,

    /// Invalid metadata
    #[error("invalid metadata")]
    Metadata {
        /// error message
        msg: String,
    },

    /// Tls support is not enabled but requested
    #[error(
        "Tls support is not enabled but requested via url, please enable it using proper feature flags"
    )]
    TlsNotSupported,
}

/// Builder for [`GrpcClient`]
#[derive(Object)]
pub struct GrpcClientBuilder {
    url: String,
    state: Mutex<Option<BuilderState>>,
}

struct AccountParam {
    verifying_key: VerifyingKey,
    signer: Arc<dyn UniffiSigner>,
}

struct MetadataParam {
    key: String,
    value: String,
}

struct MetadataBinParam {
    key: String,
    value: Vec<u8>,
}

#[derive(Default)]
struct BuilderState {
    pub account: Option<AccountState>,
    pub context: Context,
    //pub metadata: Vec<MetadataParam>,
    //pub metadata_bin: Vec<MetadataBinParam>,
    pub timeout: Option<Duration>,
}

/*
impl BuilderOp {
    fn apply<S0, S1>(self, builder: RustBuilder<S0>) -> Result<RustBuilder<S1>, GrpcClientBuilder>
    where
        S0: grpc_client_builder::State,
        S1: grpc_client_builder::State,
    {
        let new_state = match self {
            BuilderOp::PubkeyAndSigner {
                verifying_key,
                signer,
            } => builder.account(verifying_key, signer),
            BuilderOp::Metadata { key, value } => builder.metadata(&key, &value)?,
            BuilderOp::MetadataBin { key, value } => builder.metadata_bin(&key, &value)?,
            BuilderOp::Timeout(duration) => builder.timeout(duration),
        };
        Ok(new_state)
    }
}
*/

/*
impl GrpcClientBuilder {
    /// Apply given transformation to the inner builder
    fn map_builder<F>(&self, map: F)
    where
        F: FnOnce(RustBuilder) -> RustBuilder,
    {
        let mut builder_lock = self.0.lock().expect("lock poisoned");
        let builder = builder_lock.take().expect("builder must be set");
        *builder_lock = Some(map(builder));
    }
}
*/

// note: we cannot use the GrpcClient::builder() returns GrpcClientBuilder
// pattern as in rust or js, because uniffi does not support static methods
// except for constructors: https://github.com/mozilla/uniffi-rs/issues/1074
#[uniffi::export(async_runtime = "tokio")]
impl GrpcClientBuilder {
    /// Create a new builder for the provided url
    #[uniffi::constructor(name = "withUrl")]
    pub fn with_url(url: String) -> GrpcClientBuilder {
        //let builder = RustGrpcClient::builder().url(url);
        GrpcClientBuilder {
            url,
            state: Mutex::new(Some(BuilderState::default())),
        }
    }

    /// Add public key and signer to the client being built
    #[uniffi::method(name = "withPubkeyAndSigner")]
    pub fn pubkey_and_signer(
        self: Arc<Self>,
        account_pubkey: Vec<u8>,
        signer: Arc<dyn UniffiSigner>,
    ) -> Result<Arc<Self>, GrpcClientBuilderError> {
        let verifying_key = VerifyingKey::from_sec1_bytes(&account_pubkey)
            .map_err(|_| GrpcClientBuilderError::InvalidAccountPublicKey)?;
        {
            let mut ops_lock = self.state.lock().expect("lock poisoned");
            let signer = UniffiSignerBox(signer);
            ops_lock
                .as_mut()
                .ok_or(GrpcClientBuilderError::BuilderConsumed)?
                .account = Some(AccountState::new(
                verifying_key,
                BoxedDocSigner::new(signer),
            ));
        }
        Ok(self)
    }

    /// Appends ascii metadata to all requests made by the client.
    #[uniffi::method(name = "withMetadata")]
    pub fn metadata(self: Arc<Self>, key: &str, value: &str) -> Result<Arc<Self>> {
        {
            let mut ops_lock = self.state.lock().expect("lock poisoned");
            ops_lock
                .as_mut()
                .ok_or(GrpcClientBuilderError::BuilderConsumed)?
                .context
                .append_metadata(key, value)?;
        }
        Ok(self)
    }

    /// Appends binary metadata to all requests made by the client.
    ///
    /// Keys for binary metadata must have `-bin` suffix.
    #[uniffi::method(name = "withMetadataBin")]
    pub fn metadata_bin(self: Arc<Self>, key: &str, value: &[u8]) -> Result<Arc<Self>> {
        {
            let mut ops_lock = self.state.lock().expect("lock poisoned");
            ops_lock
                .as_mut()
                .ok_or(GrpcClientBuilderError::BuilderConsumed)?
                .context
                .append_metadata_bin(key, value)?;
        }
        Ok(self)
    }

    /// Sets the request timeout in milliseconds, overriding default one from the transport.
    #[uniffi::method(name = "withTimeout")]
    pub fn timeout(self: Arc<Self>, timeout_ms: u64) -> Result<Arc<Self>> {
        {
            let mut ops_lock = self.state.lock().expect("lock poisoned");
            ops_lock
                .as_mut()
                .ok_or(GrpcClientBuilderError::BuilderConsumed)?
                .timeout = Some(Duration::from_millis(timeout_ms));
        }
        Ok(self)
    }

    // this function _must_ be async despite not awaiting, so that it executes in tokio runtime
    // context
    /// Build the gRPC client.
    #[uniffi::method(name = "build")]
    pub async fn build(self: Arc<Self>) -> Result<GrpcClient, GrpcClientBuilderError> {
        let mut state_lock = self.state.lock().expect("lock poisoned");
        let BuilderState {
            account,
            context,
            timeout,
        } = state_lock
            .take()
            .ok_or(GrpcClientBuilderError::BuilderConsumed)?;
        let transport = build_transport(self.url.clone())?;

        let client = RustGrpcClient::new(context, account, transport, timeout)?;

        Ok(client.into())
    }
}

impl From<MetadataError> for GrpcClientBuilderError {
    fn from(error: MetadataError) -> Self {
        GrpcClientBuilderError::Metadata {
            msg: error.to_string(),
        }
    }
}

impl From<crate::GrpcClientBuilderError> for GrpcClientBuilderError {
    fn from(error: crate::GrpcClientBuilderError) -> Self {
        match error {
            crate::GrpcClientBuilderError::TonicTransportError(error) => {
                GrpcClientBuilderError::TonicTransportError {
                    msg: error.to_string(),
                }
            }
            crate::GrpcClientBuilderError::InvalidPrivateKey => {
                GrpcClientBuilderError::InvalidAccountPrivateKey
            }
            crate::GrpcClientBuilderError::InvalidPublicKey => {
                GrpcClientBuilderError::InvalidAccountPublicKey
            }
            crate::GrpcClientBuilderError::TransportNotSet
            | crate::GrpcClientBuilderError::MultipleTransportsSet => {
                // API above should not allow creating a builder without any transport
                unimplemented!("invalid transport setup for builder, should not happen")
            }

            crate::GrpcClientBuilderError::Metadata(err) => err.into(),
            crate::GrpcClientBuilderError::TlsNotSupported => {
                GrpcClientBuilderError::TlsNotSupported
            }
        }
    }
}
