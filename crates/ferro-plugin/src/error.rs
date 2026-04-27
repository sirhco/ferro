use thiserror::Error;

pub type PluginResult<T> = Result<T, PluginError>;

#[derive(Debug, Error)]
pub enum PluginError {
    #[error("plugin not found: {0}")]
    NotFound(String),

    #[error("invalid manifest: {0}")]
    Manifest(String),

    #[error("missing capability: {0}")]
    MissingCapability(String),

    #[error("plugin exceeded resource limits: {0}")]
    ResourceExceeded(&'static str),

    #[error("wasmtime: {0}")]
    Wasm(String),

    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("toml: {0}")]
    Toml(#[from] toml::de::Error),

    #[error("{0}")]
    Other(String),
}

impl From<wasmtime::Error> for PluginError {
    fn from(e: wasmtime::Error) -> Self {
        Self::Wasm(e.to_string())
    }
}
