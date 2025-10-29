import "../tlb/either"
import "../tlb/maybe"

struct MessageRelaxedGeneric {
    info: CommonMsgInfoRelaxed,
    init: Maybe<Either<StateInit, Cell<StateInit>>>,
    body: RemainingBitsAndRefs
}

fun MessageRelaxedGeneric.loadOpcode(self, skip_bounce: bool = false): int? {
    var slice = self.body;
    if (slice.loadBool()) {
        // actual body in the ref
        if (slice.remainingRefsCount() == 0) {
            return null;
        }
        val body = slice.loadRef();
        slice = body.beginParse();
    }

    if (slice.remainingBitsCount() < 32) {
        return null;
    }

    val opcode = slice.loadUint(32);
    if (skip_bounce && opcode == 0xFFFFFFFF) {
        if (slice.remainingBitsCount() < 32) {
            return null
        }
        return slice.loadUint(32);
    }
    return opcode;
}

@overflow1023_policy("suppress")
struct MessageRelaxed<X> {
    info: CommonMsgInfoRelaxed,
    init: Maybe<Either<StateInit, Cell<StateInit>>>,
    body: Either<X, Cell<X>>
}

fun MessageRelaxed<X>.loadBody(self): X {
    if (self.body is EitherLeft<X>) {
        return self.body.value
    }
    return self.body.value.load({
        assertEndAfterReading: false
    })
}

struct (0b0) CommonMsgInfoRelaxed {
    ihr_disabled: bool,
    bounce: bool,
    bounced: bool,
    src: address,
    dest: address,
    value: CurrencyCollection,
    extra_flags: varuint16,
    fwd_fee: coins,
    created_lt: uint64,
    created_at: uint32
}

struct CurrencyCollection {
    grams: coins,
    other: map<int32, varuint32>
}
