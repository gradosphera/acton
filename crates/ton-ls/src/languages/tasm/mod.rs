pub fn init() {
    let res = tasm_syntax::parse("PUSHINT_4 1");
    println!("{:?}", res);
}
