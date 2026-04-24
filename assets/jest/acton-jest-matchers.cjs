"use strict";

const fs = require("node:fs");
const path = require("node:path");
const util = require("node:util");

const outputPath = process.env.ACTON_JEST_MATCHERS_FILE;
if (outputPath) {
  fs.mkdirSync(path.dirname(outputPath), {recursive: true});

  const appendEvent = event => {
    try {
      fs.appendFileSync(outputPath, `${JSON.stringify(event)}\n`);
    } catch {
      // Ignore matcher logging errors to avoid impacting test execution.
    }
  };

  const inspectValue = value => {
    try {
      return util.inspect(value, {
        depth: 4,
        maxArrayLength: 50,
        maxStringLength: 500,
        breakLength: 120,
      });
    } catch {
      return "<uninspectable>";
    }
  };

  const formatError = error => {
    if (!error) {
      return "unknown matcher error";
    }
    if (typeof error === "string") {
      return error;
    }
    if (typeof error.message === "string" && error.message.length > 0) {
      return error.message;
    }
    return inspectValue(error);
  };

  const TRACE_DUMP_KIND = "transaction_dump";
  const captureAllTransactions =
    process.env.ACTON_JEST_CAPTURE_TRANSACTIONS === "1";

  let tonTestUtils;
  try {
    tonTestUtils = require("@ton/test-utils");
  } catch {
    tonTestUtils = undefined;
  }

  const normalizeValue = (value, depth = 0) => {
    if (value === null || value === undefined) {
      return value;
    }
    if (typeof value === "bigint") {
      return `${value.toString()}n`;
    }
    if (typeof value === "function") {
      return "[Function]";
    }
    if (typeof value === "string" || typeof value === "number" || typeof value === "boolean") {
      return value;
    }
    if (Array.isArray(value)) {
      return value.map(item => normalizeValue(item, depth + 1));
    }
    if (typeof value === "object") {
      if (typeof value.toString === "function") {
        const ctorName = value.constructor?.name;
        if (ctorName === "Address" || ctorName === "Cell" || ctorName === "Slice") {
          return value.toString();
        }
      }
      if (depth >= 3) {
        return inspectValue(value);
      }
      const out = {};
      for (const [key, item] of Object.entries(value)) {
        out[key] = normalizeValue(item, depth + 1);
      }
      return out;
    }
    return inspectValue(value);
  };

  const stringifyComparable = value => {
    const normalized = normalizeValue(value);
    if (typeof normalized === "string") {
      return normalized;
    }
    try {
      return JSON.stringify(normalized);
    } catch {
      return inspectValue(value);
    }
  };

  const toLtString = value => {
    if (value === null || value === undefined) {
      return undefined;
    }
    if (typeof value === "bigint") {
      return value.toString();
    }
    if (typeof value === "number" && Number.isFinite(value)) {
      return Math.trunc(value).toString();
    }
    if (typeof value === "string") {
      return value;
    }
    return undefined;
  };

  const encodeBytesAsBase64 = value => {
    if (value === null || value === undefined) {
      return undefined;
    }
    if (typeof Buffer !== "undefined" && Buffer.isBuffer(value)) {
      return value.toString("base64");
    }
    if (value instanceof Uint8Array) {
      return Buffer.from(value).toString("base64");
    }
    if (Array.isArray(value)) {
      return Buffer.from(value).toString("base64");
    }
    return undefined;
  };

  const toBocBase64 = value => {
    const direct = encodeBytesAsBase64(value);
    if (direct) {
      return direct;
    }

    if (typeof value === "string") {
      if (!value.includes("{") && /^[A-Za-z0-9+/_=-]+$/.test(value)) {
        return value;
      }
      return undefined;
    }

    if (value && typeof value.toBoc === "function") {
      const variants = [{idx: false, crc32: true}, {idx: false}, false, undefined];
      for (const variant of variants) {
        try {
          const boc = variant === undefined ? value.toBoc() : value.toBoc(variant);
          const encoded = encodeBytesAsBase64(boc);
          if (encoded) {
            return encoded;
          }
        } catch {
          // Ignore unsupported toBoc signatures.
        }
      }
    }

    return undefined;
  };

  const truncateLog = (value, maxLength = 20_000) => {
    if (typeof value !== "string" || value.length === 0) {
      return "";
    }
    if (value.length <= maxLength) {
      return value;
    }
    return `${value.slice(0, maxLength)}\n... (truncated by Acton)`;
  };

  const valuesEqual = (actual, expected) => {
    if (actual === expected) {
      return true;
    }
    if (typeof actual === "bigint" || typeof expected === "bigint") {
      return String(actual) === String(expected);
    }
    if (actual && typeof actual.equals === "function") {
      try {
        return !!actual.equals(expected);
      } catch {}
    }
    if (expected && typeof expected.equals === "function") {
      try {
        return !!expected.equals(actual);
      } catch {}
    }
    if (Array.isArray(actual) && Array.isArray(expected)) {
      if (actual.length !== expected.length) {
        return false;
      }
      for (let i = 0; i < actual.length; i++) {
        if (!valuesEqual(actual[i], expected[i])) {
          return false;
        }
      }
      return true;
    }
    return false;
  };

  const collectMismatches = (flatTransaction, cmp) => {
    const mismatches = [];
    if (!cmp || typeof cmp !== "object") {
      return mismatches;
    }

    for (const [key, expected] of Object.entries(cmp)) {
      const actual = flatTransaction?.[key];
      if (typeof expected === "function") {
        let passed = false;
        try {
          passed = !!expected(actual);
        } catch {
          passed = false;
        }
        if (!passed) {
          mismatches.push({
            field: key,
            expected: "[predicate]",
            actual: stringifyComparable(actual),
          });
        }
        continue;
      }

      if (!valuesEqual(actual, expected)) {
        mismatches.push({
          field: key,
          expected: stringifyComparable(expected),
          actual: stringifyComparable(actual),
        });
      }
    }

    return mismatches;
  };

  const flatTransactionEntries = received => {
    const subject = Array.isArray(received) ? received : [received];
    if (!tonTestUtils || typeof tonTestUtils.flattenTransaction !== "function") {
      return [];
    }

    return subject
      .map(source => {
        try {
          return {
            source,
            flat: tonTestUtils.flattenTransaction(source),
          };
        } catch {
          return undefined;
        }
      })
      .filter(Boolean);
  };

  const extractChildLts = tx => {
    if (!tx || !Array.isArray(tx.children)) {
      return [];
    }
    return tx.children
      .map(child => toLtString(child?.lt))
      .filter(value => typeof value === "string");
  };

  const pickTransactionFields = (flatTransaction, cmp) => {
    const keys = new Set([
      "lt",
      "now",
      "outMessagesCount",
      "from",
      "to",
      "on",
      "deploy",
      "aborted",
      "destroyed",
      "success",
      "exitCode",
      "actionResultCode",
      "op",
      "value",
    ]);
    if (cmp && typeof cmp === "object") {
      for (const key of Object.keys(cmp)) {
        keys.add(key);
      }
    }

    const out = {};
    for (const key of keys) {
      if (Object.hasOwn(flatTransaction, key)) {
        out[key] = normalizeValue(flatTransaction[key]);
      }
    }
    return out;
  };

  const pickActonTransactionFields = (sourceTx, flatTransaction, cmp) => ({
    lt: toLtString(sourceTx?.lt ?? flatTransaction?.lt) || "",
    raw_transaction: toBocBase64(sourceTx?.raw),
    parent_transaction: toLtString(sourceTx?.parent?.lt) || null,
    child_transactions: extractChildLts(sourceTx),
    shard_account_before: "",
    shard_account: "",
    vm_log_diff: truncateLog(sourceTx?.vmLogs),
    executor_logs: truncateLog(sourceTx?.blockchainLogs),
    actions: toBocBase64(sourceTx?.actions),
    dest_contract_info: undefined,
    flat: pickTransactionFields(flatTransaction, cmp),
  });

  const buildTransactionQueryPayload = (matcherName, received, args) => {
    if (!matcherName.endsWith("toHaveTransaction")) {
      return undefined;
    }

    const cmp = args[0];
    const entries = flatTransactionEntries(received);
    if (entries.length === 0) {
      return {
        pattern: normalizeValue(cmp),
        candidates: [],
        negated: matcherName.includes(".not."),
      };
    }

    return {
      pattern: normalizeValue(cmp),
      negated: matcherName.includes(".not."),
      candidates: entries.map(({source, flat}) => ({
        transaction: pickActonTransactionFields(source, flat, cmp),
        mismatches: collectMismatches(flat, cmp),
      })),
    };
  };

  const makeTestKey = (testPath, testName) => `${testPath}\u0000${testName}`;

  const getCurrentTestState = () => {
    const currentExpect = global.expect;
    if (!currentExpect || typeof currentExpect.getState !== "function") {
      return undefined;
    }
    const state = currentExpect.getState() || {};
    const testName = typeof state.currentTestName === "string" ? state.currentTestName : "";
    const testPath = typeof state.testPath === "string" ? state.testPath : "";
    if (!testName || !testPath) {
      return undefined;
    }
    return {
      testName,
      testPath,
    };
  };

  const traceStore = new Map();

  const recordTransactionsForCurrentTest = transactions => {
    if (!Array.isArray(transactions) || transactions.length === 0) {
      return;
    }

    const state = getCurrentTestState();
    if (!state) {
      return;
    }

    const entries = flatTransactionEntries(transactions);
    if (entries.length === 0) {
      return;
    }

    const traceTransactions = entries.map(({source, flat}) =>
      pickActonTransactionFields(source, flat, undefined),
    );
    if (traceTransactions.length === 0) {
      return;
    }

    const key = makeTestKey(state.testPath, state.testName);
    let bucket = traceStore.get(key);
    if (!bucket) {
      bucket = {
        test_name: state.testName,
        test_path: state.testPath,
        traces: [],
      };
      traceStore.set(key, bucket);
    }

    bucket.traces.push({
      transactions: traceTransactions,
    });
  };

  const flushAllTransactionTraces = () => {
    for (const entry of traceStore.values()) {
      if (!entry || !Array.isArray(entry.traces) || entry.traces.length === 0) {
        continue;
      }

      appendEvent({
        kind: TRACE_DUMP_KIND,
        matcher: "__acton_trace_dump__",
        status: "collected",
        test_name: entry.test_name,
        test_path: entry.test_path,
        transaction_traces: entry.traces,
      });
    }

    traceStore.clear();
  };

  const wrapQueueManager = queue => {
    if (!queue || queue.__actonTraceWrapped === true) {
      return queue;
    }

    const originalRunQueue = queue.runQueue;
    if (typeof originalRunQueue === "function") {
      queue.runQueue = async function (...args) {
        const out = await originalRunQueue.apply(this, args);
        if (out && Array.isArray(out.transactions)) {
          recordTransactionsForCurrentTest(out.transactions);
        }
        return out;
      };
    }

    const originalRunQueueIter = queue.runQueueIter;
    if (typeof originalRunQueueIter === "function") {
      queue.runQueueIter = function (...args) {
        const shouldCapture = args[0] === true;
        const iterator = originalRunQueueIter.apply(this, args);
        if (!iterator || typeof iterator.next !== "function") {
          return iterator;
        }

        const collected = [];
        const wrapped = {
          async next(...nextArgs) {
            const result = await iterator.next(...nextArgs);
            if (result && !result.done && result.value) {
              if (shouldCapture) {
                collected.push(result.value);
              }
            } else if (shouldCapture && collected.length > 0) {
              const txs = collected.splice(0, collected.length);
              recordTransactionsForCurrentTest(txs);
            }
            return result;
          },
          [Symbol.asyncIterator]() {
            return this;
          },
        };

        if (typeof iterator.return === "function") {
          wrapped.return = async (...returnArgs) => {
            const out = await iterator.return(...returnArgs);
            if (shouldCapture && collected.length > 0) {
              const txs = collected.splice(0, collected.length);
              recordTransactionsForCurrentTest(txs);
            }
            return out;
          };
        }

        if (typeof iterator.throw === "function") {
          wrapped.throw = (...throwArgs) => iterator.throw(...throwArgs);
        }

        return wrapped;
      };
    }

    Object.defineProperty(queue, "__actonTraceWrapped", {
      value: true,
      writable: false,
      configurable: false,
      enumerable: false,
    });

    return queue;
  };

  const patchSandboxQueueTracing = () => {
    let sandbox;
    try {
      sandbox = require("@ton/sandbox");
    } catch {
      return;
    }

    const blockchainCtor = sandbox?.Blockchain;
    if (!blockchainCtor || !blockchainCtor.prototype) {
      return;
    }

    const proto = blockchainCtor.prototype;
    if (proto.__actonQueueTracingPatched === true) {
      return;
    }

    const originalCreateQueueManager = proto.createQueueManager;
    if (typeof originalCreateQueueManager === "function") {
      proto.createQueueManager = function (...args) {
        const queue = originalCreateQueueManager.apply(this, args);
        return wrapQueueManager(queue);
      };
    }

    const originalCreate = blockchainCtor.create;
    if (typeof originalCreate === "function") {
      blockchainCtor.create = async (...args) => {
        const instance = await originalCreate.apply(blockchainCtor, args);
        if (instance && instance.defaultQueueManager) {
          wrapQueueManager(instance.defaultQueueManager);
        }
        return instance;
      };
    }

    Object.defineProperty(proto, "__actonQueueTracingPatched", {
      value: true,
      writable: false,
      configurable: false,
      enumerable: false,
    });
  };

  if (captureAllTransactions) {
    patchSandboxQueueTracing();

    if (typeof afterEach === "function") {
      afterEach(() => {
        flushAllTransactionTraces();
      });
    }

    if (typeof afterAll === "function") {
      afterAll(() => {
        flushAllTransactionTraces();
      });
    }
  }

  const getLocation = error => {
    const stack = error?.stack || new Error().stack;
    if (!stack) {
      return undefined;
    }

    const lines = stack.split("\n").slice(1);
    for (const rawLine of lines) {
      const line = rawLine.trim();
      if (
        line.includes("acton-jest-matchers.cjs") ||
        line.includes("acton-jest-setup.cjs") ||
        line.includes("/node_modules/expect/") ||
        line.includes("/node_modules/jest-")
      ) {
        continue;
      }

      const match =
        line.match(/\((.*):(\d+):(\d+)\)/) ||
        line.match(/at (.*):(\d+):(\d+)/);
      if (!match) {
        continue;
      }

      return `${match[1]}:${match[2]}:${match[3]}`;
    }
    return undefined;
  };

  const originalExpect = global.expect;
  if (
    typeof originalExpect === "function" &&
    originalExpect.__actonMatcherWrapped !== true
  ) {
    const getState = () => {
      if (typeof originalExpect.getState === "function") {
        return originalExpect.getState();
      }
      return {};
    };

    const createFailedMatcherEvent = (matcherName, received, expected, error, transactionQuery) => {
      const state = getState();
      const conciseMessage = `expect(<actual>).${matcherName}(<expected>)`;
      return {
        matcher: matcherName,
        status: "failed",
        test_name: state.currentTestName || "",
        test_path: state.testPath || "",
        received: transactionQuery ? undefined : inspectValue(received),
        expected: expected.map(stringifyComparable),
        message: transactionQuery ? conciseMessage : formatError(error),
        location: getLocation(error),
        transaction_query: transactionQuery,
      };
    };

    const wrapMatcherChain = (chain, received, matcherPath = []) => {
      if (
        chain === null ||
        chain === undefined ||
        (typeof chain !== "object" && typeof chain !== "function")
      ) {
        return chain;
      }

      return new Proxy(chain, {
        get(target, prop, receiver) {
          const value = Reflect.get(target, prop, receiver);
          if (typeof prop === "symbol") {
            return value;
          }

          if (prop === "not" || prop === "resolves" || prop === "rejects") {
            return wrapMatcherChain(value, received, matcherPath.concat(prop));
          }

          if (typeof value !== "function") {
            return value;
          }

          return (...args) => {
            const matcherName = matcherPath.concat(prop).join(".");
            const transactionQuery = buildTransactionQueryPayload(
              matcherName,
              received,
              args,
            );

            try {
              const result = value.apply(target, args);
              if (result && typeof result.then === "function") {
                return result.then(
                  resolved => resolved,
                  error => {
                    appendEvent(
                      createFailedMatcherEvent(
                        matcherName,
                        received,
                        args,
                        error,
                        transactionQuery,
                      ),
                    );
                    throw error;
                  },
                );
              }

              return result;
            } catch (error) {
              appendEvent(
                createFailedMatcherEvent(
                  matcherName,
                  received,
                  args,
                  error,
                  transactionQuery,
                ),
              );
              throw error;
            }
          };
        },
      });
    };

    const wrappedExpect = received =>
      wrapMatcherChain(originalExpect(received), received);

    Object.setPrototypeOf(wrappedExpect, Object.getPrototypeOf(originalExpect));

    for (const key of Object.getOwnPropertyNames(originalExpect)) {
      if (key === "length" || key === "name" || key === "prototype") {
        continue;
      }
      const descriptor = Object.getOwnPropertyDescriptor(originalExpect, key);
      if (descriptor) {
        Object.defineProperty(wrappedExpect, key, descriptor);
      }
    }

    Object.defineProperty(wrappedExpect, "__actonMatcherWrapped", {
      value: true,
      writable: false,
      configurable: false,
      enumerable: false,
    });

    global.expect = wrappedExpect;
  }
}
