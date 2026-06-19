import {BookOpen, Check, Copy, Link2, SquareStack} from "lucide-react"
import * as React from "react"
import {Card, CardContent, CardDescription, CardHeader, CardTitle, useToast} from "@acton/shared-ui"
import {useNavigate} from "react-router-dom"
import type {ContractABI} from "@ton/tolk-abi-to-typescript"

import type {TonClient} from "../../explorer/api/client"
import {addressKey, buildMessageNamesByOpcodeHex} from "../../explorer/api/compilerAbi"
import type {
  LocalnetNodeInfo,
  V3AccountState,
  V3TransactionListItem,
} from "../../explorer/api/types"
import {
  DeveloperAccountList,
  type DeveloperAccountListItem,
} from "../../explorer/components/DeveloperAccountList"
import {
  DeveloperTransactionList,
  type DeveloperMessageNamesByAddress,
} from "../../explorer/components/DeveloperTransactionList"
import {formatDuration} from "../../explorer/components/utils"
import {useAddressBook} from "../../explorer/hooks/useAddressBook"
import {collectRecentAccounts} from "../dashboardUtils"

import styles from "../DashboardPage.module.css"

const HOME_RECENT_TRANSACTIONS_REFRESH_MS = 2000
const HOME_NODE_INFO_REFRESH_MS = 1000

interface HomePageProps {
  readonly client: TonClient
}

interface HomeState {
  readonly transactions: readonly V3TransactionListItem[]
  readonly accountStatesByAddress: Readonly<Record<string, V3AccountState>>
  readonly isLoading: boolean
  readonly error?: string
}

export const HomePage: React.FC<HomePageProps> = ({client}) => {
  const navigate = useNavigate()
  const {showToast} = useToast()
  const {prefetchNames} = useAddressBook()
  const [nodeInfo, setNodeInfo] = React.useState<LocalnetNodeInfo | undefined>()
  const [copiedEndpoint, setCopiedEndpoint] = React.useState<string>()
  const [homeState, setHomeState] = React.useState<HomeState>({
    transactions: [],
    accountStatesByAddress: {},
    isLoading: true,
  })
  const [compilerAbiByAddress, setCompilerAbiByAddress] = React.useState<
    Map<string, ContractABI | undefined>
  >(new Map())
  const endpoints = React.useMemo(() => client.getEndpoints(), [client])
  const endpointRows = React.useMemo(
    () =>
      [
        {
          label: "V2 API",
          value: endpoints.apiV2,
          referencePath: "/api-reference/v2",
        },
        {
          label: "V3 API",
          value: endpoints.apiV3,
          referencePath: "/api-reference/v3",
        },
        {
          label: "Control API",
          value: endpoints.admin,
          referencePath: "/api-reference/control",
        },
      ].filter(endpoint => endpoint.value.length > 0),
    [endpoints],
  )
  const recentAccounts = React.useMemo(
    () => collectRecentAccounts(homeState.transactions),
    [homeState.transactions],
  )
  const recentAccountItems = React.useMemo<readonly DeveloperAccountListItem[]>(
    () =>
      recentAccounts.map(address => ({
        address,
        state: homeState.accountStatesByAddress[addressKey(address)],
      })),
    [homeState.accountStatesByAddress, recentAccounts],
  )
  const displayedAddresses = React.useMemo(() => {
    const addresses = new Set<string>()
    for (const transaction of homeState.transactions) {
      addresses.add(transaction.account)
      if (transaction.in_msg?.source) {
        addresses.add(transaction.in_msg.source)
      }
      if (transaction.in_msg?.destination) {
        addresses.add(transaction.in_msg.destination)
      }
      for (const message of transaction.out_msgs) {
        if (message.source) {
          addresses.add(message.source)
        }
        if (message.destination) {
          addresses.add(message.destination)
        }
      }
    }
    for (const account of recentAccounts) {
      addresses.add(account)
    }
    return [...addresses]
  }, [homeState.transactions, recentAccounts])
  const messageNamesByAddress = React.useMemo<DeveloperMessageNamesByAddress>(() => {
    const next = new Map<
      string,
      {
        readonly incoming: ReadonlyMap<string, string>
        readonly outgoing: ReadonlyMap<string, string>
      }
    >()
    for (const [address, abi] of compilerAbiByAddress) {
      next.set(address, {
        incoming: buildMessageNamesByOpcodeHex(abi, "incoming_messages"),
        outgoing: buildMessageNamesByOpcodeHex(abi, "outgoing_messages"),
      })
    }
    return next
  }, [compilerAbiByAddress])

  React.useEffect(() => {
    let cancelled = false
    let timeoutId: ReturnType<typeof setTimeout> | undefined

    const loadNodeInfo = async () => {
      try {
        const nextNodeInfo = await client.getNodeInfo()
        if (!cancelled) {
          setNodeInfo(nextNodeInfo)
        }
      } catch {
        if (!cancelled) {
          setNodeInfo(undefined)
        }
      } finally {
        if (!cancelled) {
          timeoutId = globalThis.setTimeout(() => void loadNodeInfo(), HOME_NODE_INFO_REFRESH_MS)
        }
      }
    }

    void loadNodeInfo()

    return () => {
      cancelled = true
      if (timeoutId !== undefined) {
        globalThis.clearTimeout(timeoutId)
      }
    }
  }, [client])

  React.useEffect(() => {
    let cancelled = false
    let timeoutId: ReturnType<typeof setTimeout> | undefined

    const loadHomeState = async (showLoading: boolean) => {
      if (showLoading) {
        setHomeState(current => ({
          ...current,
          isLoading: true,
          error: undefined,
        }))
      }

      try {
        const transactionsResponse = await client.getRecentTransactions(8)
        const transactions = transactionsResponse.transactions
        const accounts = collectRecentAccounts(transactions)
        let accountStatesByAddress: Record<string, V3AccountState> = {}

        if (accounts.length > 0) {
          try {
            const accountStates = await client.getAccountStates(accounts, false)
            accountStatesByAddress = Object.fromEntries(
              accountStates.accounts.map(account => [addressKey(account.address), account]),
            )
          } catch (error) {
            console.error("Failed to fetch recent account states", error)
          }
        }

        if (!cancelled) {
          setHomeState({
            transactions,
            accountStatesByAddress,
            isLoading: false,
          })
        }
      } catch (error) {
        if (!cancelled) {
          const message = error instanceof Error ? error.message : "Failed to load dashboard"
          setHomeState(current => ({
            transactions: current.transactions,
            accountStatesByAddress: current.accountStatesByAddress,
            isLoading: false,
            error: current.transactions.length === 0 ? message : undefined,
          }))
        }
      } finally {
        if (!cancelled) {
          timeoutId = globalThis.setTimeout(
            () => void loadHomeState(false),
            HOME_RECENT_TRANSACTIONS_REFRESH_MS,
          )
        }
      }
    }

    void loadHomeState(true)

    return () => {
      cancelled = true
      if (timeoutId !== undefined) {
        globalThis.clearTimeout(timeoutId)
      }
    }
  }, [client])

  React.useEffect(() => {
    void prefetchNames(displayedAddresses)
  }, [displayedAddresses, prefetchNames])

  React.useEffect(() => {
    let isActive = true

    const loadMessageNames = async () => {
      if (displayedAddresses.length === 0) {
        setCompilerAbiByAddress(new Map())
        return
      }

      const states = await client.getAccountStates(displayedAddresses, false).catch(error => {
        console.error("Failed to fetch transaction account states", error)
        return undefined
      })

      if (!isActive) {
        return
      }

      const addressToCodeHash = new Map<string, string>()
      for (const account of states?.accounts ?? []) {
        if (account.code_hash) {
          addressToCodeHash.set(addressKey(account.address), account.code_hash)
        }
      }

      const codeHashes = [...new Set(addressToCodeHash.values())]
      const fetchedAbis =
        codeHashes.length > 0
          ? await client
              .getCompilerAbis(codeHashes)
              .catch((): Awaited<ReturnType<TonClient["getCompilerAbis"]>> => ({}))
          : {}

      if (!isActive) {
        return
      }

      const abiByCodeHash = new Map<string, ContractABI | undefined>()
      for (const codeHash of codeHashes) {
        abiByCodeHash.set(codeHash, fetchedAbis[codeHash]?.compiler_abi)
      }

      const next = new Map<string, ContractABI | undefined>()
      for (const address of displayedAddresses) {
        const key = addressKey(address)
        const codeHash = addressToCodeHash.get(key)
        next.set(key, codeHash ? abiByCodeHash.get(codeHash) : undefined)
      }
      setCompilerAbiByAddress(next)
    }

    void loadMessageNames()

    return () => {
      isActive = false
    }
  }, [client, displayedAddresses])

  React.useEffect(() => {
    if (!copiedEndpoint) {
      return
    }

    const timeoutId = globalThis.setTimeout(() => setCopiedEndpoint(undefined), 2000)
    return () => {
      globalThis.clearTimeout(timeoutId)
    }
  }, [copiedEndpoint])

  const copyEndpoint = React.useCallback(
    async (endpoint: string) => {
      try {
        await navigator.clipboard.writeText(endpoint)
        setCopiedEndpoint(endpoint)
      } catch (error) {
        console.error("Failed to copy endpoint", error)
        showToast({
          variant: "error",
          title: "Copy failed",
          description: "Failed to copy endpoint URL.",
        })
      }
    },
    [showToast],
  )

  return (
    <>
      <section className={styles.hero}>
        <div>
          <h1 className={styles.title}>Home</h1>
          <p className={styles.subtitle}>
            A quick snapshot of your local node and recent activity.
          </p>
        </div>
      </section>

      <section className={styles.homeLayout}>
        <div className={styles.homeTopRow}>
          <Card className={`${styles.dashboardCard} ${styles.homeCard}`}>
            <CardHeader className={styles.dashboardCardHeader}>
              <div className={styles.cardTitleRow}>
                <div className={styles.cardIcon}>
                  <SquareStack size={16} />
                </div>
                <div>
                  <CardTitle className={styles.dashboardCardTitle}>Current block</CardTitle>
                  <CardDescription className={styles.dashboardCardDescription}>
                    Latest masterchain sequence number.
                  </CardDescription>
                </div>
              </div>
            </CardHeader>
            <CardContent className={styles.dashboardCardContent}>
              <div className={styles.metricValue}>
                {nodeInfo ? `#${nodeInfo.last_block_seqno}` : "—"}
              </div>
              <div className={styles.metricMeta}>
                {nodeInfo
                  ? `${formatDuration(nodeInfo.uptime_seconds)} uptime`
                  : "Waiting for node info"}
              </div>
            </CardContent>
          </Card>

          <Card className={`${styles.dashboardCard} ${styles.homeCard}`}>
            <CardHeader className={styles.dashboardCardHeader}>
              <div className={styles.cardTitleRow}>
                <div className={styles.cardIcon}>
                  <Link2 size={16} />
                </div>
                <div>
                  <CardTitle className={styles.dashboardCardTitle}>Endpoints</CardTitle>
                  <CardDescription className={styles.dashboardCardDescription}>
                    Active local URLs for the current node.
                  </CardDescription>
                </div>
              </div>
            </CardHeader>
            <CardContent className={`${styles.dashboardCardContent} ${styles.endpointList}`}>
              {endpointRows.map(endpoint => {
                const isCopied = copiedEndpoint === endpoint.value

                return (
                  <div key={endpoint.label} className={styles.endpointRow}>
                    <span className={styles.endpointText}>
                      <span className={styles.endpointLabel}>{endpoint.label}</span>
                      <span className={styles.endpointValue}>{endpoint.value}</span>
                    </span>
                    <span className={styles.endpointActions}>
                      <button
                        type="button"
                        className={`${styles.endpointButton} ${isCopied ? styles.endpointButtonCopied : ""}`}
                        aria-label={
                          isCopied ? "Endpoint copied" : `Copy ${endpoint.label} endpoint`
                        }
                        title={isCopied ? "Copied" : "Copy endpoint"}
                        onClick={() => void copyEndpoint(endpoint.value)}
                      >
                        {isCopied ? <Check size={14} /> : <Copy size={14} />}
                      </button>
                      <button
                        type="button"
                        className={styles.endpointButton}
                        aria-label={`Open ${endpoint.label} reference`}
                        title="Open API reference"
                        onClick={() => void navigate(endpoint.referencePath)}
                      >
                        <BookOpen size={14} />
                      </button>
                    </span>
                  </div>
                )
              })}
            </CardContent>
          </Card>
        </div>

        {homeState.error ? (
          <div className={`${styles.homeTransactionsCard} ${styles.emptyState}`}>
            {homeState.error}
          </div>
        ) : homeState.isLoading ? (
          <div className={`${styles.homeTransactionsCard} ${styles.skeletonList}`}>
            {[0, 1, 2, 3].map(index => (
              <div key={index} className={styles.skeletonRow}>
                <div className={styles.skeletonMain}>
                  <span className={`${styles.skeletonLine} ${styles.skeletonLinePrimary}`} />
                  <span className={`${styles.skeletonLine} ${styles.skeletonLineSecondary}`} />
                </div>
                <span className={`${styles.skeletonLine} ${styles.skeletonLineMeta}`} />
              </div>
            ))}
          </div>
        ) : homeState.transactions.length === 0 ? (
          <div className={`${styles.homeTransactionsCard} ${styles.emptyState}`}>
            No transactions yet.
          </div>
        ) : (
          <DeveloperTransactionList
            className={styles.homeTransactionsCard}
            title="Recent transactions"
            transactions={homeState.transactions}
            messageNamesByAddress={messageNamesByAddress}
            onTransactionClick={hashHex => {
              void navigate(`/explorer/tx/${encodeURIComponent(hashHex)}`)
            }}
            onAddressClick={address => {
              void navigate(`/explorer/address/${encodeURIComponent(address)}`)
            }}
          />
        )}

        <div className={styles.homeMainColumn}>
          {homeState.error ? (
            <div className={styles.emptyState}>{homeState.error}</div>
          ) : homeState.isLoading ? (
            <div className={styles.skeletonList} aria-label="Loading accounts">
              {[0, 1, 2, 3].map(index => (
                <div key={index} className={styles.skeletonRow}>
                  <div className={styles.skeletonMain}>
                    <span className={`${styles.skeletonLine} ${styles.skeletonLinePrimary}`} />
                    <span className={`${styles.skeletonLine} ${styles.skeletonLineSecondary}`} />
                  </div>
                  <span className={`${styles.skeletonLine} ${styles.skeletonLineMeta}`} />
                </div>
              ))}
            </div>
          ) : (
            <DeveloperAccountList
              title="Recent accounts"
              accounts={recentAccountItems}
              onAddressClick={address => {
                void navigate(`/explorer/address/${encodeURIComponent(address)}`)
              }}
            />
          )}
        </div>
      </section>
    </>
  )
}
