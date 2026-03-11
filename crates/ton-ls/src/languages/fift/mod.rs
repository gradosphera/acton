pub fn init() {
    let res = fift_syntax::parse("");
    println!("{:?}", res);
}
