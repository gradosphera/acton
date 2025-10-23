type Either<X, Y> = EitherLeft<X> | EitherRight<Y>

struct (0b0) EitherLeft<X> {
    value: X
}

struct (0b1) EitherRight<X> {
    value: X
}
