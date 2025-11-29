use crate::support::{ProjectBuilder, TestOutputExt};

const SIMPLE_CONTRACT: &str = r#"
fun onInternalMessage(in: InMessage) {}
fun onBouncedMessage(_: InMessageBounced) {}

get fun currentCounter(): int { return 0 }
get fun currentCounter2(arg: int): int { return arg }
get fun currentCounter3(arg: int): int { return arg + 10 }
get fun getCell(): cell { return beginCell().storeInt(32, 32).endCell() }
"#;

const TEST_PREPARE: &str = r#"
import "../../lib/testing/expect"
import "../../lib/build/build"
import "../../lib/io"
import "../../lib/emulation/network"
import "../../lib/fmt"

struct Counter {
    address: address
    init: ContractState
}

fun Counter.fromStorage() {
    val init = ContractState {
        code: build("simple"),
        data: createEmptyCell(),
    };
    val address = AutoDeployAddress { stateInit: init }.calculateAddress();
    return Counter { address, init }
}

fun setupTest() {
    val counter = Counter.fromStorage();

    val deployer = net.treasury("deployer");
    val msg = createMessage({
        bounce: false,
        value: ton("1.0"),
        dest: {
            stateInit: counter.init,
        },
    });

    net.send(deployer.address, msg, SEND_MODE_PAY_FEES_SEPARATELY);
    return (counter, deployer)
}
"#;

#[test]
fn test_unknown_get_method_call() {
    ProjectBuilder::new("simple")
        .contract("simple", SIMPLE_CONTRACT)
        .test_file(
            "test",
            (TEST_PREPARE.to_string()
                + r#"

            get fun `test-foo`() {
                val (counter, deployer) = setupTest();

                val counterRes = net.runGetMethod<int, tuple>(counter.address, "currentCounter999");
                println(format1("Counter: {}", counterRes));
            }
        "#)
            .as_str(),
        )
        .build()
        .acton()
        .test()
        .run()
        .failure()
        .assert_snapshot_matches("integration/snapshots/test_unknown_get_method_call.stdout.txt");
}

#[test]
fn test_get_method_call_return_type_mismatch() {
    // TODO: fow now we cannot check this
    ProjectBuilder::new("simple")
        .contract("simple", SIMPLE_CONTRACT)
        .test_file(
            "test",
            (TEST_PREPARE.to_string()
                + r#"

            get fun `test-foo`() {
                val (counter, deployer) = setupTest();

                val counterRes = net.runGetMethod<address, tuple>(counter.address, "getCell");
                println(format1("Counter: {}", counterRes));
            }
        "#)
            .as_str(),
        )
        .build()
        .acton()
        .test()
        .run()
        .success()
        .assert_snapshot_matches(
            "integration/snapshots/test_get_method_call_return_type_mismatch.stdout.txt",
        );
}

#[test]
fn test_no_arg_get_method_call() {
    ProjectBuilder::new("simple")
        .contract("simple", SIMPLE_CONTRACT)
        .test_file(
            "test",
            (TEST_PREPARE.to_string()
                + r#"

            get fun `test-foo`() {
                val (counter, deployer) = setupTest();

                val counterRes = net.runGetMethod<int, tuple>(counter.address, "currentCounter2");
                println(format1("Counter: {}", counterRes));
            }
        "#)
            .as_str(),
        )
        .build()
        .acton()
        .test()
        .run()
        .failure()
        .assert_snapshot_matches("integration/snapshots/test_no_arg_get_method_call.stdout.txt");
}

#[test]
fn test_no_arg_get_method_call_2() {
    ProjectBuilder::new("simple")
        .contract("simple", SIMPLE_CONTRACT)
        .test_file(
            "test",
            (TEST_PREPARE.to_string()
                + r#"

            get fun `test-foo`() {
                val (counter, deployer) = setupTest();

                val counterRes = net.runGetMethod<int, tuple>(counter.address, "currentCounter3");
                println(format1("Counter: {}", counterRes));
            }
        "#)
            .as_str(),
        )
        .build()
        .acton()
        .test()
        .run()
        .failure()
        .assert_snapshot_matches("integration/snapshots/test_no_arg_get_method_call_2.stdout.txt");
}
