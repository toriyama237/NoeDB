//! OpenTelemetry trace export (optional `otel` feature).

use opentelemetry::global;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::trace::SdkTracerProvider;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Install a global `tracing` subscriber that exports spans to OTLP/gRPC.
///
/// # Errors
///
/// Exporter or subscriber setup failures.
pub(crate) fn init(endpoint: &str) -> Result<(), String> {
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint)
        .build()
        .map_err(|e| e.to_string())?;

    let provider = SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .build();

    global::set_tracer_provider(provider.clone());
    let tracer = provider.tracer("noedb");

    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,noedb_engine=debug")),
        )
        .with(tracing_opentelemetry::layer().with_tracer(tracer))
        .init();

    Ok(())
}
