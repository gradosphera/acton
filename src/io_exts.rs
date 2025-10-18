use core::ffi::c_char;
use emulator::executor::Executor;
use emulator::get_executor::GetExecutor;
use emulator::tuple::stack::{Tuple, TupleItem};
use emulator::{extension, pop_args, register_ext_methods};
use std::sync::Mutex;

static TEST_OUTPUT_BUFFER: Mutex<String> = Mutex::new(String::new());
static TEST_STDERR_BUFFER: Mutex<String> = Mutex::new(String::new());
static CAPTURE_TEST_OUTPUT: Mutex<bool> = Mutex::new(false);

pub fn start_capturing_test_output() {
    *CAPTURE_TEST_OUTPUT.lock().unwrap() = true;
    *TEST_OUTPUT_BUFFER.lock().unwrap() = String::new();
    *TEST_STDERR_BUFFER.lock().unwrap() = String::new();
}

pub fn stop_capturing_test_output() -> (String, String) {
    *CAPTURE_TEST_OUTPUT.lock().unwrap() = false;
    (
        TEST_OUTPUT_BUFFER.lock().unwrap().clone(),
        TEST_STDERR_BUFFER.lock().unwrap().clone(),
    )
}

pub fn is_capturing_test_output() -> bool {
    *CAPTURE_TEST_OUTPUT.lock().unwrap()
}

extension!(print, (s: TupleItem, type_name: String), print_impl);
fn print_impl(_stack: &mut Tuple, (s, type_name): (TupleItem, String)) {
    let typed_tuple = if let TupleItem::Tuple(tuple) = &s {
        TupleItem::TypedTuple {
            type_name,
            items: tuple.clone(),
        }
    } else {
        s
    };
    if is_capturing_test_output() {
        TEST_OUTPUT_BUFFER
            .lock()
            .unwrap()
            .push_str(&format!("{}\n", typed_tuple));
    } else {
        println!("{}", typed_tuple);
    }
}

extension!(eprint, (s: String), eprint_impl);
fn eprint_impl(_stack: &mut Tuple, (s,): (String,)) {
    if is_capturing_test_output() {
        TEST_STDERR_BUFFER
            .lock()
            .unwrap()
            .push_str(&format!("{}\n", s));
    } else {
        eprintln!("{}", s);
    }
}

pub fn register_extensions(executor: &mut Executor) {
    register_ext_methods!(executor, {
        1 => print,
        2 => eprint,
    });
}

pub fn register_get_extensions(executor: &mut GetExecutor) {
    register_ext_methods!(executor, {
        1 => print,
        2 => eprint,
    });
}
