import "assert"
import "expect"
import "../fmt"
import "../ffi/ffi"
import "../types/out_actions"

fun Expectation<OutActionList>.toBeNonEmpty(self) {
    expect(self.value.size(), self.location).toNotEqual(0);
}

fun Expectation<OutActionList>.toBeEmpty(self) {
    expect(self.value.size(), self.location).toEqual(0);
}

fun Expectation<OutActionList>.toBeSendMessageAt<Msg>(self, index: int) {
    val action = self.value.at(index);
    if (action is OutActionSendMessage) {
        try {
            action.loadMessage<Msg>();
        } catch (e) {
            if (e == 63) {
                // prefix mismatch
                val msg = action.loadGenericMessage();
                val opcode = msg.loadOpcode();
                if (opcode == null) {
                    impl.fail_to_find_out_msg<Msg>(e, self.location);
                    return;
                }

                val opcode_type_name = ffi.type_name_by_opcode(opcode);

                Assert.fail(
                    format4(
                        "Out action is a send message, but it cannot be load as a `{}` (opcode: 0x{:x}) since body has opcode 0x{:x}{}",
                        Msg.__typeName2(),
                        Msg.getDeclaredPackPrefix(),
                        opcode,
                        opcode_type_name == null ? "" : format1(" ({})", opcode_type_name),
                    ),
                    self.location
                );
                return;
            }
            impl.fail_to_find_out_msg<Msg>(e, self.location);
        }
        return;
    }

    Assert.fail(
        format2("Out action at index {} is not a send message, but {}", index, action.kind()),
        self.location
    );
}

struct impl

fun impl.fail_to_find_out_msg<Msg>(exit_code: int, location: slice) {
    Assert.fail(
        format2(
            "Out action is a send message, but it cannot be load as a `{}` due to exit code {}",
            Msg.__typeName2(),
            exit_code
        ),
        location
    );
}
