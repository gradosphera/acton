import "expect"
import "../types/out_actions"

fun Expectation<OutActionList>.toBeNonEmpty(self) {
    expect(self.value.size(), self.location).toNotEqual(0)
}
