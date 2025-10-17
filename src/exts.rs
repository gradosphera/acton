use crate::executor::Executor;
use crate::exts_lib::{
    Tuple, pop_two_tuples_and_equal, push_bool_as_int, push_string, take_last_string, with_tuple,
};
use crate::stack_serialization::TupleItem;
use crate::{TESTS, ext, register_ext_methods};
use core::ffi::c_char;

ext!(print, |t: &mut Tuple| {
    if let Some(s) = take_last_string(t) {
        println!("{}", s);
    }
});

ext!(eprint, |t: &mut Tuple| {
    if let Some(s) = take_last_string(t) {
        eprintln!("{}", s);
    }
});

ext!(read_file, |t: &mut Tuple| {
    if let Some(path) = take_last_string(t) {
        match std::fs::read_to_string(&path) {
            Ok(content) => push_string(t, &content),
            Err(_) => t.push(TupleItem::Null),
        }
    }
});

ext!(assert_equal, |t: &mut Tuple| {
    match pop_two_tuples_and_equal(t) {
        Some(eq) => push_bool_as_int(t, eq),
        None => {
            eprintln!("Assertion failed: incompatible values for comparison");
            push_bool_as_int(t, false);
        }
    }
});

ext!(register_test, |t: &mut Tuple| {
    if let Some(name) = take_last_string(t) {
        TESTS.lock().unwrap().push(name);
    }
});

pub fn register_extensions(executor: &mut Executor) {
    register_ext_methods!(executor, {
        1 => print,
        2 => eprint,
        3 => read_file,
        4 => assert_equal,
        5 => register_test,
    });
}
