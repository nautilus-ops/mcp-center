use std::error::Error;
use tokio::runtime::Runtime;
use tokio_util::sync::CancellationToken;
use crate::app::config::McpCenter;

/// The Application trait defines the standard lifecycle for long-running services.
///
/// This trait provides a common interface for applications that need to:
/// - Load configuration and initialize resources during startup
/// - Run continuously until shutdown is requested
/// - Handle graceful shutdown scenarios
///
/// The trait is designed to work with async/await patterns and supports
/// cancellation-based shutdown mechanisms.
///
/// # Lifecycle
///
/// 1. **Preparation Phase**: `prepare()` is called first to load configuration
///    and initialize any required resources or connections.
/// 2. **Execution Phase**: `run()` is called to start the main application loop
///    which continues until a shutdown signal is received.
///
/// # Thread Safety
///
/// Implementations must be `Send + Sync` to support multi-threaded async runtimes.
///
pub trait Application: Send + Sync {
    fn prepare(&mut self, config: McpCenter) -> Result<(), Box<dyn Error>>;

    /// Runs the main application loop until shutdown is requested.
    ///
    /// This method contains the core business logic of the application and should:
    /// - Execute the main service functionality (e.g., processing requests, monitoring resources)
    /// - Monitor the shutdown token and respond to cancellation requests
    /// - Handle operational errors gracefully without terminating the service
    /// - Perform periodic maintenance tasks if required
    ///
    /// The method should run indefinitely until the shutdown token is cancelled,
    /// at which point it should perform cleanup and return gracefully.
    ///
    /// # Arguments
    ///
    /// * `shutdown` - Cancellation token that signals when the application should shut down
    ///
    /// # Returns
    ///
    /// * `Result<(), Box<dyn Error>>` - Ok(()) on successful shutdown, Error for critical failures
    ///
    /// # Errors
    ///
    /// This method should return an error only for critical failures that prevent
    /// the application from continuing:
    /// - Unrecoverable system errors
    /// - Critical resource exhaustion
    /// - Security violations
    ///
    /// Transient errors (network timeouts, temporary service unavailability) should
    /// be handled internally with appropriate retry/backoff strategies.
    ///
    /// # Shutdown Handling
    ///
    /// Implementations must monitor the `shutdown` token and respond promptly:
    ///
    /// # Implementation Notes
    ///
    /// - Use `tokio::select!` to handler both work and shutdown signals
    /// - Implement proper cleanup in the shutdown path
    /// - Consider implementing health checks and metrics collection
    /// - Use structured logging for operational visibility
    /// - Handle backpressure and rate limiting as appropriate
    fn run(&mut self, shutdown: CancellationToken, rt: Runtime) -> Result<(), Box<dyn Error>>;
}
