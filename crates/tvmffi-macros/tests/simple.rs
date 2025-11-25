use num_bigint::BigInt;
use tonlib_core::cell::ArcCell;
use tvmffi_macros::TupleSerialize;
use tycho_types::models::IntAddr;

#[derive(TupleSerialize)]
struct TestStruct {
    cell: ArcCell,
    int: BigInt,
    flag: bool,
    i: i32,
    u: u32,
    addr: IntAddr,
}

#[test]
fn simple_struct_to_tuple() {
    let cell = ArcCell::default();
    let int = BigInt::from(123i32);
    let flag = true;
    let i = -5;
    let u = 42u32;
    let addr = IntAddr::default();

    let s = TestStruct {
        cell: cell.clone(),
        int: int.clone(),
        flag,
        i,
        u,
        addr,
    };

    let tuple = s.to_tuple();

    // Тут уже проверяем количество элементов и их содержание
    assert_eq!(tuple.0.len(), 6);
}
