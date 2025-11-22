use abi::{ContractAbi, contract_abi};
use anyhow::anyhow;
use dap::events::Event;
use dap::responses::ContinueResponse;
use dap_client::DapClient;
use emulator::AnyExecutor;
use emulator::blockchain::Blockchain;
use emulator::emulator::Emulator;
use emulator::executor::ExecutorVerbosity;
use emulator::get_executor::{GetMethodParams, GetMethodResult};
use emulator::step_get_executor::StepGetExecutor;
use emulator_rs::config::ActonConfig;
use emulator_rs::context::{
    AssertsContext, BuildCache, BuildContext, ChainContext, Context, DebugCtx, Emulations, Env,
    IoContext, KnownAddresses,
};
use emulator_rs::debugger::debug_context::DebugContext;
use emulator_rs::file_build_cache::FileBuildCache;
use emulator_rs::formatter::FormatterContext;
use emulator_rs::{debugger, ffi};
use owo_colors::OwoColorize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::thread::JoinHandle;
use std::time::Duration;
use std::{env, fs, thread};
use tasm::printer::FormatOptions;
use tolkc::CompilerResult;
use tolkc::source_map::SourceMap;
use tonlib_core::TonAddress;
use tonlib_core::cell::{ArcCell, CellBuilder};
use tonlib_core::tlb_types::tlb::TLB;
use tvmffi::stack::Tuple;
use tycho_types::boc::Boc;

pub struct DebuggerClient {
    client: DapClient,
}

impl DebuggerClient {
    pub fn connect(address: &str) -> anyhow::Result<Self> {
        let mut client = DapClient::connect(address)?;
        client.start()?;
        client.initialize()?;
        wait_for_initialized(&mut client)?;
        client.configuration_done()?;
        client.launch()?;
        wait_for_stopped(&mut client)?;

        Ok(Self { client })
    }

    pub fn step_in(&mut self, thread_id: i64) -> anyhow::Result<()> {
        self.client.step_in(thread_id)
    }

    pub fn continue_execution(&mut self, thread_id: i64) -> anyhow::Result<ContinueResponse> {
        self.client.continue_execution(thread_id)
    }

    pub fn step_over(&mut self, thread_id: i64) -> anyhow::Result<()> {
        self.client.step_over(thread_id)
    }

    pub fn step_out(&mut self, thread_id: i64) -> anyhow::Result<()> {
        self.client.step_out(thread_id)
    }

    pub fn stack_trace(&mut self, thread_id: i64) -> anyhow::Result<Vec<SourcePosition>> {
        let trace = self.client.stack_trace(thread_id)?;
        let positions = trace
            .stack_frames
            .iter()
            .filter_map(|frame| {
                frame.source.as_ref().and_then(|source| {
                    source.path.as_ref().map(|path| {
                        SourcePosition::new(path.clone(), frame.line as u32, frame.column as u32)
                    })
                })
            })
            .collect();
        Ok(positions)
    }

    pub fn variables(&mut self, thread_id: i64) -> anyhow::Result<Vec<dap::types::Variable>> {
        let variables = self.client.variables(thread_id)?;
        Ok(variables.variables)
    }

    pub fn assert_position(
        &mut self,
        thread_id: i64,
        expected: &SourcePosition,
    ) -> anyhow::Result<()> {
        let positions = self.stack_trace(thread_id)?;
        let actual = positions
            .first()
            .ok_or_else(|| anyhow!("No stack frames available"))?;

        println!("Position: {}", actual);

        if actual == expected {
            Ok(())
        } else {
            Err(anyhow!(
                "Position mismatch: expected {}, got {}",
                expected,
                actual
            ))
        }
    }

    pub fn terminate(&mut self) -> anyhow::Result<()> {
        self.client.terminate()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SourcePosition {
    pub file: String,
    pub line: u32,
    pub column: u32,
}

impl SourcePosition {
    pub fn new(file: String, line: u32, column: u32) -> Self {
        Self {
            file: normalize_path(&file),
            line,
            column,
        }
    }
}

impl std::fmt::Display for SourcePosition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}:{}", self.file, self.line, self.column)
    }
}

fn normalize_path(path: &str) -> String {
    let path_buf = PathBuf::from(path);
    if let Ok(current_dir) = env::current_dir()
        && let Ok(relative) = path_buf.strip_prefix(&current_dir)
    {
        return relative.to_string_lossy().to_string();
    }
    path.to_string()
}

fn wait_for_initialized(client: &mut DapClient) -> anyhow::Result<()> {
    loop {
        if let Ok(Some(event)) = client.try_receive_event(Duration::from_secs(1))
            && matches!(event, Event::Initialized)
        {
            break;
        }
    }
    Ok(())
}

#[test]
fn test_debugging() -> anyhow::Result<()> {
    let code = "
global foo: int;

fun main() {
    foo = 100;
    if (foo > 10) {
        foo = 200;
    }
    return foo
}
    ";

    let (mut client, _) = prepare_test(code)?;

    let positions = client.stack_trace(1)?;
    let initial_pos = positions.first().expect("unexpected empty stack");
    println!("Initial position: {}", initial_pos);

    for i in 0..40 {
        client.step_in(1)?;
        let positions = client.stack_trace(1)?;
        let pos = positions.first().unwrap();
        println!("Step {}: {}", i + 4, pos);

        let variables = client.variables(1)?;
        println!("Variables: {:?}", variables);
    }

    client.terminate()?;
    Ok(())
}

#[test]
fn test_can_continue_execution() -> anyhow::Result<()> {
    let code = "
global foo: int;

fun main() {
    foo = 100;
    if (foo > 10) {
        foo = 300;
    }
    return foo
}
    ";

    let (mut client, handle) = prepare_test(code)?;

    client.continue_execution(1)?;

    handle
        .join()
        .expect("failed to join thread with debug execution");

    Ok(())
}

fn prepare_test(code: &str) -> anyhow::Result<(DebuggerClient, JoinHandle<()>)> {
    fs::write("script2.tolk", code)?;

    let code = code.to_owned();
    let handle = thread::spawn(move || {
        let result = run_script_file("script2.tolk", code.as_str(), 42069).expect("");
        println!("{result}");
    });

    thread::sleep(Duration::from_millis(1000));
    let address = "127.0.0.1:42069".to_string();
    let client = DebuggerClient::connect(&address)?;
    Ok((client, handle))
}

fn wait_for_stopped(client: &mut DapClient) -> anyhow::Result<()> {
    loop {
        if let Ok(Some(event)) = client.try_receive_event(Duration::from_millis(100))
            && matches!(event, Event::Stopped(_))
        {
            return Ok(());
        }
    }
}

fn run_script_file(file_path: &str, content: &str, debug_port: u16) -> anyhow::Result<String> {
    let abi = contract_abi(content, file_path);

    match tolkc::compile(Path::new(file_path), true) {
        CompilerResult::Success(result) => {
            let code_cell = ArcCell::from_boc_b64(&result.code_boc64)?;
            let data_cell = ArcCell::default();

            fs::write(
                "out.source_map.json",
                serde_json::to_string(&result.source_map)?,
            )?;

            let disasm = tasm::decompile::Disassembler::new();
            let code = disasm.decompile_cell(&Boc::decode_base64(&result.code_boc64)?)?;
            fs::write(
                "out.disasm.txt",
                code.print(&FormatOptions {
                    show_offsets: true,
                    show_hashes: true,
                    source_map: None,
                }),
            )?;
            fs::write("out.disasm.fif", result.fift_code)?;
            fs::write("out.boc", code_cell.to_boc(false)?)?;

            let (script_result, ctx, formatter) = execute_script(
                &code_cell,
                &data_cell,
                &abi,
                &result.source_map.unwrap_or(Default::default()),
                debug_port,
                ExecutorVerbosity::FullLocationStackVerbose,
            )?;
            get_script_result(script_result, ctx, formatter)
        }
        CompilerResult::Error(error) => {
            anyhow::bail!("Cannot compile script file {}", error.message)
        }
    }
}

fn execute_script<'a>(
    code_cell: &ArcCell,
    data_cell: &ArcCell,
    abi: &ContractAbi,
    source_map: &SourceMap,
    debug_port: u16,
    verbosity: ExecutorVerbosity,
) -> anyhow::Result<(GetMethodResult, IoContext, FormatterContext)> {
    let dest_address = contract_address(code_cell)?;

    let params = GetMethodParams {
        code: code_cell.to_boc_b64(false)?.to_string(),
        data: data_cell.to_boc_b64(false)?.to_string(),
        verbosity,
        libs: "".to_string(),
        address: dest_address.to_string(),
        unixtime: 0,
        balance: "10".to_string(),
        rand_seed: "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
        gas_limit: "0".to_string(),
        method_id: 0,
        debug_enabled: true,
        extra_currencies: HashMap::new(),
        prev_blocks_info: None,
    };

    let mut emulator = Emulator::new(verbosity);
    let mut blockchain = Blockchain::new(None, None);
    let mut build_cache = BuildCache::new();
    let mut file_build_cache =
        FileBuildCache::new(None).expect("Failed to create file cache for script execution");
    let mut known_addresses = KnownAddresses::new();
    let mut known_code_cell = HashMap::new();
    let mut emulations = Emulations::new();

    let mut assert_failure = None;
    let mut expected_exit_code = None;

    let mut ctx = Context {
        env: Env {
            config: &ActonConfig::load()?,
            abi,
            default_log_level: verbosity,
        },
        io: IoContext {
            stdout_buffer: "".to_string(),
            stderr_buffer: "".to_string(),
            capture_output: true,
        },
        asserts: AssertsContext {
            assert_failure: &mut assert_failure,
            expected_exit_code: &mut expected_exit_code,
        },
        chain: ChainContext {
            blockchain: &mut blockchain,
            emulator: &mut emulator,
            emulations: &mut emulations,
        },
        build: BuildContext {
            build_cache: &mut build_cache,
            file_build_cache: &mut file_build_cache,
            known_addresses: &mut known_addresses,
            known_code_cells: &mut known_code_cell,
            need_debug_info: false,
            backtrace: None,
        },
        debug: DebugCtx::Disabled,
    };

    let mut executor = StepGetExecutor::new(Tuple::empty(), params.clone());
    ffi::register(&mut executor, &mut ctx);

    let transport = debugger::start_dap_server(debug_port);

    let mut dbg_ctx = DebugContext::new(
        transport,
        AnyExecutor::Get(executor.clone()),
        source_map,
        "main".to_string(),
    );

    ctx.debug = DebugCtx::new(&mut dbg_ctx);

    executor.prepare_get_method(0, Tuple::empty());

    ctx.debug.ctx().process_incoming_requests(true)?;

    let result = executor.finish_get_method(&params.code);
    let formatter = FormatterContext::from_context(&ctx);
    let io = ctx.io;
    Ok((result, io, formatter))
}

fn get_script_result(
    result: GetMethodResult,
    io: IoContext,
    formatter: FormatterContext,
) -> anyhow::Result<String> {
    match &result {
        GetMethodResult::Success(result) => {
            let cell = ArcCell::from_boc_b64(&result.stack)?;

            let tuple = Tuple::deserialize(&cell)?;
            let tuple_str = formatter.format_tuple(&tuple);

            Ok(tuple_str + io.stdout_buffer.as_str() + io.stderr_buffer.as_str())
        }
        GetMethodResult::Error(error) => Ok(format!(
            "{} {}",
            "Execution error:".red(),
            error.error.red()
        )),
    }
}

fn contract_address(code: &ArcCell) -> anyhow::Result<TonAddress> {
    let state_init = CellBuilder::new()
        .store_bit(false)
        .map_err(|e| anyhow!("Failed to store bounce flag: {}", e))?
        .store_bit(false)
        .map_err(|e| anyhow!("Failed to store maybe libraries: {}", e))?
        .store_ref_cell_optional(Some(code))
        .map_err(|e| anyhow!("Failed to store code cell: {}", e))?
        .store_ref_cell_optional(Some(&ArcCell::default()))
        .map_err(|e| anyhow!("Failed to store data cell: {}", e))?
        .store_bit(false)
        .map_err(|e| anyhow!("Failed to store maybe tick/tock: {}", e))?
        .build()
        .map_err(|e| anyhow!("Failed to build state init cell: {}", e))?;

    let dest_address = TonAddress::new(0, state_init.cell_hash());
    Ok(dest_address)
}

const CODE: &str = "
        global a: int;

        struct (0x1) Msg {
            a: int32,
            b: int64,
        }

        struct (0x2) Msg2 {
            a: int64,
            b: int16,
        }

        type AnyMsg = Msg | Msg2;

        @inline_ref
        fun ref_func(b: int) {
            return b + 10
        }

        @noinline
        fun get_tuple(a: int, b: int) {
            return [a, b]
        }

        fun main() {
            val cell = Msg { a: 10, b: 11 }.toCell();
            a = 200;
            var b = a;

            val msg = lazy AnyMsg.fromCell(cell);

            val [c, d] = get_tuple(a, b);

            match (msg) {
            	Msg => {
                    b = msg.a;
                    ref_func(b);
                    debug.print(b);
                    b += msg.b;
            	}
            	Msg2 => {
                    a = 2;
            	}
            	else => {
                    a = 3;
            	}
            }

            var builder = beginCell();

            a = 101;
            if (a > 100) {
                a = 200;
                builder.storeUint(32, 32);
            }

            match (a) {
                10 => {
                    a = 20;
                }
                200 => {
                    a = 300
                }
            }

            match (a) {
                10 => {
                    a = 20;
                }
                201 => {
                    a = 300
                }
                else => {
                    a = 400
                }
            }

            repeat(10) {
                a += 1;
                builder.storeUint(a, 32);
            }

            return a + b + c + d;
        }
";
