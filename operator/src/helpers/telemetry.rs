use crate::{Error, Result};

pub fn get_trace_id() -> String {
    use opentelemetry::trace::TraceContextExt;
    use tracing_opentelemetry::OpenTelemetrySpanExt;

    tracing::Span::current()
        .context()
        .span()
        .span_context()
        .trace_id()
        .to_hex()
}
