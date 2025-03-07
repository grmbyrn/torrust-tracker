//! Application jobs launchers.
//!
//! The main application setup has only two main stages:
//!
//! 1. Setup the domain layer: the core tracker.
//! 2. Launch all the application services as concurrent jobs.
//!
//! This modules contains all the functions needed to start those jobs.
pub mod health_check_api;
pub mod http_tracker;
pub mod torrent_cleanup;
pub mod tracker_apis;
pub mod udp_tracker;

/// This is the message that the "launcher" spawned task sends to the main
/// application process to notify the service was successfully started.
///
#[derive(Debug)]
pub struct Started {
    pub address: std::net::SocketAddr,
}

pub async fn make_rust_tls(enabled: bool, tsl_config: &TslConfig) -> Option<Result<RustlsConfig, Error>> {
    if !enabled {
        info!("TLS not enabled");
        return None;
    }

    if let (Some(cert), Some(key)) = (tsl_config.ssl_cert_path.clone(), tsl_config.ssl_key_path.clone()) {
        info!("Using https: cert path: {cert}.");
        info!("Using https: key path: {key}.");

        Some(
            RustlsConfig::from_pem_file(cert, key)
                .await
                .map_err(|err| Error::BadTlsConfig {
                    source: (Arc::new(err) as DynError).into(),
                }),
        )
    } else {
        Some(Err(Error::MissingTlsConfig {
            location: Location::caller(),
        }))
    }
}

#[cfg(test)]
mod tests {

    use camino::Utf8PathBuf;
    use torrust_tracker_configuration::TslConfig;

    use super::{make_rust_tls, Error};

    #[tokio::test]
    async fn it_should_error_on_bad_tls_config() {
        let err = make_rust_tls(
            true,
            &TslConfig {
                ssl_cert_path: Some(Utf8PathBuf::from("bad cert path")),
                ssl_key_path: Some(Utf8PathBuf::from("bad key path")),
            },
        )
        .await
        .expect("tls_was_enabled")
        .expect_err("bad_cert_and_key_files");

        assert!(matches!(err, Error::BadTlsConfig { source: _ }));
    }

    #[tokio::test]
    async fn it_should_error_on_missing_cert_or_key_paths() {
        let err = make_rust_tls(
            true,
            &TslConfig {
                ssl_cert_path: None,
                ssl_key_path: None,
            },
        )
        .await
        .expect("tls_was_enabled")
        .expect_err("missing_config");

        assert!(matches!(err, Error::MissingTlsConfig { location: _ }));
    }
}

use std::panic::Location;
use std::sync::Arc;

use axum_server::tls_rustls::RustlsConfig;
use thiserror::Error;
use torrust_tracker_configuration::TslConfig;
use torrust_tracker_located_error::{DynError, LocatedError};
use tracing::info;

/// Error returned by the Bootstrap Process.
#[derive(Error, Debug)]
pub enum Error {
    /// Enabled tls but missing config.
    #[error("tls config missing")]
    MissingTlsConfig { location: &'static Location<'static> },

    /// Unable to parse tls Config.
    #[error("bad tls config: {source}")]
    BadTlsConfig {
        source: LocatedError<'static, dyn std::error::Error + Send + Sync>,
    },
}
