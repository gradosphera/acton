use crate::integration::check::run_simple_test;
use function_name::named;

#[test]
#[named]
fn test_check_no_bounce_handler() {
    run_simple_test(
        "no_bounce_handler",
        r#"
            fun sendReply(dest: address) {
                val reply = createMessage({
                    bounce: BounceMode.Only256BitsOfBody,
                    value: ton("0.1"),
                    dest,
                });
                reply.send(SEND_MODE_REGULAR);
            }

            fun onInternalMessage(in: InMessage) {
                sendReply(in.senderAddress);
            }
        "#,
        function_name!(),
    )
}
