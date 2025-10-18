use crate::context::Context;
use emulator::executor::Executor;
use emulator::get_executor::GetExecutor;
use emulator::tuple::stack::Tuple;
use emulator::{extension, pop_args, register_ext_methods};
use std::sync::Mutex;

#[derive(Debug, Clone)]
pub struct AssertFailure {
    pub left: Tuple,
    pub left_type: String,
    pub right: Tuple,
    pub right_type: String,
    pub message: Option<String>,
    pub location: Option<String>,
}

static LAST_ASSERT_FAILURE: Mutex<Option<AssertFailure>> = Mutex::new(None);

pub fn get_last_assert_failure() -> Option<AssertFailure> {
    LAST_ASSERT_FAILURE.lock().unwrap().clone()
}

pub fn clear_last_assert_failure() {
    *LAST_ASSERT_FAILURE.lock().unwrap() = None;
}

extension!(assert_equal, Context, (location: String, message: String, right: Tuple, right_name: String, left: Tuple, left_name: String), assert_equal_impl);
fn assert_equal_impl(
    _ctx: &mut Context,
    stack: &mut Tuple,
    location: String,
    message: String,
    right: Tuple,
    right_name: String,
    left: Tuple,
    left_name: String,
) {
    if left == right {
        stack.push_bool_as_int(true);
    } else {
        *LAST_ASSERT_FAILURE.lock().unwrap() = Some(AssertFailure {
            left,
            right,
            left_type: left_name,
            right_type: right_name,
            message: Some(message),
            location: Some(location),
        });
        stack.push_bool_as_int(false);
    }
}

pub fn register_extensions(executor: &mut Executor, ctx: *mut std::ffi::c_void) {
    register_ext_methods!(executor, ctx, {
        4 => assert_equal,
    });
}

pub fn register_get_extensions(executor: &mut GetExecutor, ctx: *mut std::ffi::c_void) {
    register_ext_methods!(executor, ctx, {
        4 => assert_equal,
    });
}
