struct (0b0) None {
}

struct (0b1) Value<X> {
    value: X
}

type Maybe<X> = Value<X> | None
