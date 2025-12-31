use num_bigint::BigInt;
use tvmffi::from_stack::DeserializationOptions;
use tvmffi::stack::TupleItem;
use tvmffi::to_stack::SerializationOptions;
use tvmffi_macros::{TupleDeserialize, TupleSerialize};

#[derive(TupleSerialize, TupleDeserialize, Debug, PartialEq)]
struct OptionStruct {
    val: Option<i32>,
}

#[test]
fn test_option_some() {
    let s = OptionStruct { val: Some(42) };
    let tuple = s.to_tuple(SerializationOptions::default()).unwrap();

    assert_eq!(tuple.0.len(), 1);
    match &tuple.0[0] {
        TupleItem::Int(i) => assert_eq!(*i, BigInt::from(42)),
        _ => panic!("Expected Int"),
    }

    let s2 = OptionStruct::from_tuple(&tuple, DeserializationOptions::default()).unwrap();
    assert_eq!(s, s2);
}

#[test]
fn test_option_none() {
    let s = OptionStruct { val: None };
    let tuple = s.to_tuple(SerializationOptions::default()).unwrap();

    assert_eq!(tuple.0.len(), 1);
    match &tuple.0[0] {
        TupleItem::Null => {}
        _ => panic!("Expected Null"),
    }

    let s2 = OptionStruct::from_tuple(&tuple, DeserializationOptions::default()).unwrap();
    assert_eq!(s, s2);
}

#[derive(TupleDeserialize, Debug, PartialEq)]
struct ExtraStruct {
    val: i32,
}

#[test]
fn test_allow_extra() {
    let tuple = tvmffi::stack::Tuple(vec![
        TupleItem::Int(BigInt::from(42)),
        TupleItem::Int(BigInt::from(100)),
    ]);

    let options = DeserializationOptions {
        allow_extra: true,
        ..Default::default()
    };
    let s = ExtraStruct::from_tuple(&tuple, options).expect("Should allow extra");
    assert_eq!(s.val, 42);
}

#[test]
fn test_disallow_extra() {
    #[derive(TupleDeserialize, Debug, PartialEq)]
    struct NoExtraStruct {
        val: i32,
    }

    let tuple = tvmffi::stack::Tuple(vec![
        TupleItem::Int(BigInt::from(42)),
        TupleItem::Int(BigInt::from(100)),
    ]);

    let res = NoExtraStruct::from_tuple(&tuple, DeserializationOptions::default());
    assert!(res.is_err());
}

#[derive(TupleDeserialize, Debug, PartialEq)]
struct MissingStruct {
    val: Option<i32>,
}

#[test]
fn test_allow_missing() {
    let tuple = tvmffi::stack::Tuple(vec![]);

    let options = DeserializationOptions {
        allow_missing: true,
        ..Default::default()
    };
    let s = MissingStruct::from_tuple(&tuple, options).expect("Should allow missing");
    assert_eq!(s.val, None);
}

#[test]
fn test_disallow_missing() {
    #[derive(TupleDeserialize, Debug, PartialEq)]
    struct NoMissingStruct {
        val: Option<i32>,
    }

    let tuple = tvmffi::stack::Tuple(vec![]);

    let res = NoMissingStruct::from_tuple(&tuple, DeserializationOptions::default());
    assert!(res.is_err());
}
