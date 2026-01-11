export enum TestStatus {
  Passed = "Passed",
  Failed = "Failed",
  Skipped = "Skipped",
  Todo = "Todo",
}

export interface TestReport {
  readonly name: string
  readonly suite_name: string
  readonly file_path: string
  readonly row: number
  readonly column: number
  readonly status: TestStatus
  readonly message?: string
  readonly trace_path?: string
}

export interface BackendTransaction {
  readonly lt: string
  readonly raw_transaction: string
  readonly parent_transaction: string | null
  readonly child_transactions: string[]
  readonly shard_account_before: string
  readonly shard_account: string
  readonly vm_log_diff: string
  readonly executor_logs: string
  readonly actions?: string
  readonly dest_contract_info?: string
}

export interface TransactionList {
  readonly transactions: BackendTransaction[]
}

export interface Trace {
  readonly name: string
  readonly traces: TransactionList[]
  readonly contracts: string[]
  readonly wallets: Record<string, string>
}

export interface AbiMessage {
  readonly name: string
  readonly opcode: number | undefined
}

export interface Abi {
  readonly messages: AbiMessage[]
  readonly exitCodes?: Record<number, string>
}

export interface BackendContractInfo {
  readonly name: string
  readonly code_boc64: string
  readonly source_map: unknown
  readonly abi?: Abi
}

export interface SourceLocation {
  readonly file: string
  readonly line: number
  readonly column: number
  readonly end_line: number
  readonly end_column: number
}

export interface DebugLocation {
  readonly idx: number
  readonly loc: SourceLocation
}

export interface TraceStepExecute {
  readonly type: "execute"
  readonly instr: string
  readonly stack: string
  readonly offset: number
  readonly hash: string
  readonly gas: number
}

export interface TraceStepException {
  readonly type: "exception"
  readonly errno: string
  readonly message: string
  readonly handled: boolean
}

export interface TraceStepFinalC5 {
  readonly type: "final_c5"
  readonly cell: string
}

export type TraceStep = TraceStepExecute | TraceStepException | TraceStepFinalC5

export interface HighLevelTraceStepMapped {
  readonly type: "mapped"
  readonly inner: TraceStep
  readonly locs: DebugLocation[]
}

export interface HighLevelTraceStepUnmapped {
  readonly type: "unmapped"
  readonly inner: TraceStep
}

export type HighLevelTraceStep = HighLevelTraceStepMapped | HighLevelTraceStepUnmapped

export interface HighLevelTrace {
  readonly steps: HighLevelTraceStep[]
}
