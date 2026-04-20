// these error codes are quite strange, but they are exactly the same as in FunC implementation

enum Errors {
    NotFromAdmin = 401
    NotFromOwner = 401
    NotFromCollection = 405
    BatchLimitExceeded = 399
    InvalidItemIndex = 402
    InvalidWorkchain = 333
    TooSmallRestAmount = 402
    InvalidMessage = 0xFFFF
}
