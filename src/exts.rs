use crate::executor::Executor;
use crate::exts_lib::Tuple;
use crate::stack_serialization::TupleItem;
use crate::{TESTS, ext_args, pop_args, register_ext_methods};
use core::ffi::c_char;

ext_args!(print, (s: String), |_t: &mut Tuple, (s,)| {
    println!("{}", s);
});

ext_args!(eprint, (s: String), |_t: &mut Tuple, (s,)| {
    eprintln!("{}", s);
});

ext_args!(read_file, (path: String), |t: &mut Tuple, (path,)| {
    match std::fs::read_to_string(&path) {
        Ok(content) => t.push_string(&content),
        Err(_) => t.push(TupleItem::Null),
    }
});

ext_args!(assert_equal, (left: Tuple, right: Tuple), |t: &mut Tuple, (left, right)| {
    if left == right {
        t.push_bool_as_int(true);
    } else {
        eprintln!("Assertion failed: incompatible values for comparison");
        t.push_bool_as_int(false);
    }
});

ext_args!(register_test, (name: String), |_t: &mut Tuple, (name,)| {
    TESTS.lock().unwrap().push(name);
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
