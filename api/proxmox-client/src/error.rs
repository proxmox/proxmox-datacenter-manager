use std::any::Any;
use std::fmt::{self, Display};

/// For error types provided by the user of this crate.
pub trait Error: Sized + Display + fmt::Debug + Any + Send + Sync + 'static {
    /// An arbitrary error message.
    fn custom<T: Display>(msg: T) -> Self;

    /// Successfully queried the status of a task, and the task has failed.
    fn task_failed<T: Display>(msg: T) -> Self {
        Self::custom(format!("task failed: {msg}"))
    }

    /// An API call returned an error status.
    fn api_error<T: Display>(status: http::StatusCode, msg: T) -> Self {
        Self::custom(format!("api error (status = {status}): {msg}"))
    }

    /// The API behaved unexpectedly.
    fn bad_api<T: Display>(msg: T) -> Self {
        Self::custom(msg)
    }

    /// The environment returned an error or bad data.
    fn env<T: Display>(msg: T) -> Self {
        Self::custom(msg)
    }

    /// A second factor was required, but the [`Environment`](crate::Environment) did not provide
    /// an implementation to get it.
    fn second_factor_not_supported() -> Self {
        Self::custom("not supported")
    }

    /// There was an error building an [`http::Uri`].
    fn uri(err: http::Error) -> Self {
        Self::custom(err)
    }

    /// A generic internal error such as a serde_json serialization error.
    fn internal<T: Display>(err: T) -> Self {
        Self::custom(err)
    }

    /// An API call which requires authorization was attempted without logging in first.
    fn unauthorized() -> Self {
        Self::custom("unauthorized")
    }

    /// An extended client call required the ability to "pause" while polling API endpoints.
    /// (Mostly to wait for "tasks" to finish.), and no implementation for this was provided.
    fn sleep_not_supported() -> Self {
        Self::custom("no async 'sleep' implementation available")
    }
}

impl Error for anyhow::Error {
    fn custom<T: Display>(msg: T) -> Self {
        anyhow::format_err!("{msg}")
    }
}
