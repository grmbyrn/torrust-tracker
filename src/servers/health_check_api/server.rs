//! Logic to run the Health Check HTTP API server.
//!
//! This API is intended to be used by the container infrastructure to check if
//! the whole application is healthy.
use std::net::SocketAddr;
use std::time::Duration;

use axum::http::HeaderName;
use axum::response::Response;
use axum::routing::get;
use axum::{Json, Router};
use axum_server::Handle;
use futures::Future;
use hyper::Request;
use serde_json::json;
use tokio::sync::oneshot::{Receiver, Sender};
use tower_http::compression::CompressionLayer;
use tower_http::propagate_header::PropagateHeaderLayer;
use tower_http::request_id::{MakeRequestUuid, SetRequestIdLayer};
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use tracing::{debug, Level, Span};

use crate::bootstrap::jobs::Started;
use crate::servers::health_check_api::handlers::health_check_handler;
use crate::servers::registar::ServiceRegistry;
use crate::servers::signals::{graceful_shutdown, Halted};

/// Starts Health Check API server.
///
/// # Panics
///
/// Will panic if binding to the socket address fails.
pub fn start(
    bind_to: SocketAddr,
    tx: Sender<Started>,
    rx_halt: Receiver<Halted>,
    register: ServiceRegistry,
) -> impl Future<Output = Result<(), std::io::Error>> {
    let router = Router::new()
        .route("/", get(|| async { Json(json!({})) }))
        .route("/health_check", get(health_check_handler))
        .with_state(register)
        .layer(CompressionLayer::new())
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .layer(PropagateHeaderLayer::new(HeaderName::from_static("x-request-id")))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
                .on_request(|request: &Request<axum::body::Body>, _span: &Span| {
                    let method = request.method().to_string();
                    let uri = request.uri().to_string();
                    let request_id = request
                        .headers()
                        .get("x-request-id")
                        .map(|v| v.to_str().unwrap_or_default())
                        .unwrap_or_default();

                    tracing::span!(
                        target: "HEALTH CHECK API",
                        tracing::Level::INFO, "request", method = %method, uri = %uri, request_id = %request_id);
                })
                .on_response(|response: &Response, latency: Duration, _span: &Span| {
                    let status_code = response.status();
                    let request_id = response
                        .headers()
                        .get("x-request-id")
                        .map(|v| v.to_str().unwrap_or_default())
                        .unwrap_or_default();
                    let latency_ms = latency.as_millis();

                    tracing::span!(
                        target: "HEALTH CHECK API",
                        tracing::Level::INFO, "response", latency = %latency_ms, status = %status_code, request_id = %request_id);
                }),
        )
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid));

    let socket = std::net::TcpListener::bind(bind_to).expect("Could not bind tcp_listener to address.");
    let address = socket.local_addr().expect("Could not get local_addr from tcp_listener.");

    let handle = Handle::new();

    debug!(target: "HEALTH CHECK API", "Starting service with graceful shutdown in a spawned task ...");

    tokio::task::spawn(graceful_shutdown(
        handle.clone(),
        rx_halt,
        format!("Shutting down http server on socket address: {address}"),
    ));

    let running = axum_server::from_tcp(socket)
        .handle(handle)
        .serve(router.into_make_service_with_connect_info::<SocketAddr>());

    tx.send(Started { address })
        .expect("the Health Check API server should not be dropped");

    running
}
