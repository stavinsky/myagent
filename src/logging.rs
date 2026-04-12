use tracing_subscriber::fmt;
use tracing::Level;

/// Initialize the centralized logging system
/// 
/// Logs are output to stderr with format: [LEVEL] message
/// No timestamps, just level and message for clean output
pub fn init(log_level: Level) {
    fmt()
        .with_target(false)
        .with_thread_ids(false)
        .with_level(true)
        .with_ansi(true)
        .with_max_level(log_level)
        .with_writer(std::io::stderr)
        .with_thread_names(false)
        .with_line_number(false)
        .without_time()
        .init();
}
