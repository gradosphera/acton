use core::ffi::{c_char, c_void};
use serde::{Serialize, Serializer};

/// Verbosity level for the executor logs.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub enum ExecutorVerbosity {
    /// Minimal logging.
    #[default]
    Short = 0,
    /// Detailed logging.
    Full = 1,
    /// Logging with location information.
    FullLocation = 2,
    /// Logging with location and gas consumption.
    FullLocationGas = 3,
    /// Logging with location and stack state.
    FullLocationStack = 4,
    /// Extremely detailed logging with location, stack, and more.
    FullLocationStackVerbose = 5,
}

impl Serialize for ExecutorVerbosity {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_i8(*self as i8)
    }
}

/// Callback type for custom extension methods (external opcodes).
///
/// # Arguments
///
/// * `ctx`   — User-defined context.
/// * `stack` — The current TVM stack, encoded as a Base64 BoC string.
///
/// # Returns
///
/// Must return the new stack as a Base64 BoC string. If the stack is not modified,
/// return the original `stack` pointer.
pub type ExtMethodCallback<Ctx = c_void> =
    unsafe extern "C" fn(ctx: *mut Ctx, stack: *const c_char) -> *const c_char;
