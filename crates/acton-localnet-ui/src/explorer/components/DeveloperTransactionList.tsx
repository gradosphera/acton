import type React from "react"

import {addressKey} from "../api/compilerAbi"
import type {Message, Transaction, V3Message, V3TransactionListItem} from "../api/types"

import {AddressLabel} from "./AddressLabel"
import {formatNano, formatTimeAgo, hashToHex} from "./utils"

import styles from "./DeveloperTransactionList.module.css"

export type DeveloperTransaction = Transaction | V3TransactionListItem

type DeveloperMessage = Message | V3Message
export type DeveloperMessageNamesByAddress = ReadonlyMap<
  string,
  {
    readonly incoming: ReadonlyMap<string, string>
    readonly outgoing: ReadonlyMap<string, string>
  }
>

type DeveloperEndpoint =
  | {
      readonly kind: "address"
      readonly address: string
      readonly fallback: string
    }
  | {readonly kind: "text"; readonly label: string; readonly title?: string}

interface DeveloperTransactionRow {
  readonly key: string
  readonly transaction: DeveloperTransaction
  readonly time: number
  readonly from: DeveloperEndpoint
  readonly to: DeveloperEndpoint
  readonly direction: "IN" | "OUT"
  readonly messageName?: string
  readonly valueLabel: string
  readonly isSuccess: boolean
  readonly statusLabel: string
}

interface DeveloperTransactionListProps {
  readonly transactions: readonly DeveloperTransaction[]
  readonly className?: string
  readonly title?: string
  readonly emptyState?: React.ReactNode
  readonly maxRows?: number
  readonly messageNamesByAddress?: DeveloperMessageNamesByAddress
  readonly onTransactionClick?: (hashHex: string, transaction: DeveloperTransaction) => void
  readonly onAddressClick?: (address: string) => void
}

export const DeveloperTransactionListSkeleton: React.FC<{
  readonly className?: string
  readonly title?: string
  readonly rows?: number
}> = ({className, title, rows = 5}) => (
  <div
    className={`${styles.tableWrap} ${className ?? ""}`}
    aria-label={title ? `Loading ${title}` : "Loading transactions"}
  >
    {title ? <div className={styles.tableTitle}>{title}</div> : null}
    <table className={styles.table}>
      <thead>
        <tr>
          <th className={styles.timeHeader}>Time</th>
          <th className={styles.fromHeader}>From</th>
          <th className={styles.directionHeader} aria-label="Direction" />
          <th>To</th>
          <th className={styles.opcodeHeader}>Opcode</th>
          <th className={styles.valueHeader}>Value</th>
        </tr>
      </thead>
      <tbody>
        {Array.from({length: rows}, (_, index) => (
          <tr key={`developer-transaction-skeleton-${index}`} className={styles.row}>
            <td className={styles.timeCell}>
              <span className={`${styles.skeletonLine} ${styles.skeletonTime}`} />
            </td>
            <td className={`${styles.addressCell} ${styles.fromCell}`}>
              <span className={`${styles.skeletonLine} ${styles.skeletonAddress}`} />
            </td>
            <td className={styles.directionCell}>
              <span className={`${styles.skeletonLine} ${styles.skeletonDirection}`} />
            </td>
            <td className={styles.addressCell}>
              <span className={`${styles.skeletonLine} ${styles.skeletonAddress}`} />
            </td>
            <td className={styles.opcodeCell}>
              <span className={`${styles.skeletonLine} ${styles.skeletonOpcode}`} />
            </td>
            <td className={styles.valueCell}>
              <span className={`${styles.skeletonLine} ${styles.skeletonValue}`} />
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  </div>
)

export const DeveloperTransactionList: React.FC<DeveloperTransactionListProps> = ({
  transactions,
  className,
  title,
  emptyState = "No transactions yet.",
  maxRows,
  messageNamesByAddress,
  onTransactionClick,
  onAddressClick,
}) => {
  const allRows = transactions.flatMap(transaction =>
    buildDeveloperRows(transaction, messageNamesByAddress),
  )
  const rows = maxRows === undefined ? allRows : allRows.slice(0, maxRows)

  if (rows.length === 0) {
    return <div className={`${styles.emptyState} ${className ?? ""}`}>{emptyState}</div>
  }

  return (
    <div className={`${styles.tableWrap} ${className ?? ""}`}>
      {title ? <div className={styles.tableTitle}>{title}</div> : null}
      <table className={styles.table}>
        <thead>
          <tr>
            <th className={styles.timeHeader}>Time</th>
            <th className={styles.fromHeader}>From</th>
            <th className={styles.directionHeader} aria-label="Direction" />
            <th>To</th>
            <th className={styles.opcodeHeader}>Opcode</th>
            <th className={styles.valueHeader}>Value</th>
          </tr>
        </thead>
        <tbody>
          {rows.map(row => {
            const hashHex = hashToHex(row.transaction.hash)
            const canOpenTransaction = hashHex !== undefined && onTransactionClick !== undefined
            const timeTitle = formatAbsoluteTime(row.time)

            return (
              <tr
                key={row.key}
                className={`${styles.row} ${canOpenTransaction ? styles.rowInteractive : ""}`}
                onClick={() => {
                  if (hashHex) {
                    onTransactionClick?.(hashHex, row.transaction)
                  }
                }}
                title={row.statusLabel}
              >
                <td className={styles.timeCell}>
                  <span title={timeTitle}>{formatTimeAgo(row.time)}</span>
                </td>
                <td className={`${styles.addressCell} ${styles.fromCell}`}>
                  <EndpointCell endpoint={row.from} onAddressClick={onAddressClick} />
                </td>
                <td className={styles.directionCell}>
                  <span
                    className={`${styles.directionBadge} ${
                      row.direction === "IN" ? styles.directionIn : styles.directionOut
                    }`}
                  >
                    {row.direction}
                  </span>
                </td>
                <td className={styles.addressCell}>
                  <EndpointCell endpoint={row.to} onAddressClick={onAddressClick} />
                </td>
                <td className={styles.opcodeCell}>
                  <span className={styles.opcodeValue}>{row.messageName ?? "—"}</span>
                </td>
                <td className={styles.valueCell}>
                  <span className={styles.valueText}>{row.valueLabel}</span>
                </td>
              </tr>
            )
          })}
        </tbody>
      </table>
    </div>
  )
}

const EndpointCell: React.FC<{
  readonly endpoint: DeveloperEndpoint
  readonly onAddressClick?: (address: string) => void
}> = ({endpoint, onAddressClick}) => {
  if (endpoint.kind === "text") {
    return (
      <span className={styles.endpointText} title={endpoint.title}>
        {endpoint.label}
      </span>
    )
  }

  if (!onAddressClick) {
    return (
      <span className={styles.endpointText}>
        <AddressLabel address={endpoint.address} fallback={endpoint.fallback} />
      </span>
    )
  }

  return (
    <button
      type="button"
      className={styles.addressButton}
      onClick={event => {
        event.stopPropagation()
        onAddressClick(endpoint.address)
      }}
    >
      <AddressLabel address={endpoint.address} fallback={endpoint.fallback} />
    </button>
  )
}

function buildDeveloperRows(
  transaction: DeveloperTransaction,
  messageNamesByAddress?: DeveloperMessageNamesByAddress,
): DeveloperTransactionRow[] {
  const rows: DeveloperTransactionRow[] = []
  const time = getTransactionTime(transaction)
  const account = transaction.account
  const isSuccess = isTransactionSuccess(transaction)
  const statusLabel = getTransactionStatusLabel(transaction)

  transaction.out_msgs.forEach((message, index) => {
    const to = addressEndpoint(message.destination, "External")
    rows.push({
      key: `${transaction.hash}:out:${message.hash || index}`,
      transaction,
      time,
      from: addressEndpoint(message.source || account, "Account"),
      to,
      direction: "OUT",
      messageName: resolveMessageLabel(message, messageNamesByAddress),
      valueLabel: formatMessageValue(message, to),
      isSuccess,
      statusLabel,
    })
  })

  if (transaction.in_msg) {
    const from = addressEndpoint(transaction.in_msg.source, "External")
    rows.push({
      key: `${transaction.hash}:in`,
      transaction,
      time,
      from,
      to: addressEndpoint(transaction.in_msg.destination || account, "Account"),
      direction: "IN",
      messageName: resolveMessageLabel(transaction.in_msg, messageNamesByAddress),
      valueLabel: formatMessageValue(transaction.in_msg, from),
      isSuccess,
      statusLabel,
    })
  }

  if (rows.length === 0) {
    rows.push({
      key: `${transaction.hash}:empty`,
      transaction,
      time,
      from: textEndpoint("External"),
      to: addressEndpoint(account, "Account"),
      direction: "IN",
      valueLabel: "—",
      isSuccess,
      statusLabel,
    })
  }

  return rows
}

function getTransactionTime(transaction: DeveloperTransaction): number {
  return "now" in transaction ? transaction.now : transaction.utime
}

function isTransactionSuccess(transaction: DeveloperTransaction): boolean {
  if ("description" in transaction) {
    return (
      !transaction.description.aborted &&
      transaction.description.compute_ph.success &&
      transaction.description.action.success
    )
  }

  return transaction.success
}

function getTransactionStatusLabel(transaction: DeveloperTransaction): string {
  if (isTransactionSuccess(transaction)) {
    return "Confirmed transaction"
  }

  if ("description" in transaction) {
    return `Failed transaction, exit ${transaction.description.compute_ph.exit_code}`
  }

  return `Failed transaction, exit ${transaction.exit_code}`
}

function addressEndpoint(address: string | undefined, fallback: string): DeveloperEndpoint {
  return address ? {kind: "address", address, fallback} : textEndpoint(fallback)
}

function textEndpoint(label: string, title?: string): DeveloperEndpoint {
  return {kind: "text", label, title}
}

function parseNanoValue(value: string | number | undefined): bigint {
  if (value === undefined) {
    return 0n
  }

  try {
    return BigInt(value)
  } catch {
    return 0n
  }
}

function formatDeveloperValue(value: bigint): string {
  return `${formatNano(value.toString())} GRAM`
}

function formatMessageValue(
  message: DeveloperMessage,
  externalEndpoint: DeveloperEndpoint,
): string {
  if (externalEndpoint.kind === "text" && externalEndpoint.label === "External") {
    return "—"
  }

  return formatDeveloperValue(parseNanoValue(message.value))
}

function formatMessageOpcode(message: DeveloperMessage | undefined): string | undefined {
  if (!message || !("opcode" in message)) {
    return undefined
  }

  return formatOpcode(message.opcode)
}

function formatOpcode(opcode: string | number | undefined): string | undefined {
  if (opcode === undefined) {
    return undefined
  }

  const normalized = typeof opcode === "string" ? opcode.trim() : opcode
  const value =
    typeof normalized === "number"
      ? normalized
      : normalized.startsWith("0x") || normalized.startsWith("0X")
        ? Number.parseInt(normalized.slice(2), 16)
        : Number.parseInt(normalized, 10)

  if (!Number.isInteger(value) || value < 0 || value > 0xff_ff_ff_ff) {
    return undefined
  }

  return `0x${value.toString(16).padStart(8, "0")}`
}

function resolveMessageName(
  message: DeveloperMessage | undefined,
  messageNamesByAddress?: DeveloperMessageNamesByAddress,
): string | undefined {
  if (!message || !messageNamesByAddress) {
    return undefined
  }

  const opcode = formatMessageOpcode(message)
  if (!opcode) {
    return undefined
  }

  const destinationNames = message.destination
    ? messageNamesByAddress.get(addressKey(message.destination))
    : undefined
  const sourceNames = message.source
    ? messageNamesByAddress.get(addressKey(message.source))
    : undefined

  return destinationNames?.incoming.get(opcode) ?? sourceNames?.outgoing.get(opcode) ?? undefined
}

function resolveMessageLabel(
  message: DeveloperMessage | undefined,
  messageNamesByAddress?: DeveloperMessageNamesByAddress,
): string | undefined {
  return resolveMessageName(message, messageNamesByAddress) ?? formatMessageOpcode(message)
}

function formatAbsoluteTime(utime: number): string {
  return new Date(utime * 1000).toLocaleString(undefined, {
    day: "2-digit",
    month: "short",
    year: "numeric",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  })
}
