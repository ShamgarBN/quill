//! Local-only telemetry. We do NOT phone home, ever.
//!
//! Two layers:
//! 1. `tracing` for structured logs to stderr (dev) or unified macOS log (prod).
//! 2. The cloud-call audit log lives separately in `services::llm` (Phase 2+)
//!    so it persists to disk in JSON-lines.

pub fn init() {
    // Honor RUST_LOG when set; otherwise default to info for our crate.
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,quill_desktop_lib=debug"));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .compact()
        .init();
}
