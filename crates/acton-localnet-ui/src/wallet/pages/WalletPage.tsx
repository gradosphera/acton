import * as React from "react"
import {useEffect, useMemo, useState} from "react"
import {Link} from "react-router-dom"

import {
  ArrowUpRight,
  Check,
  Copy,
  Import,
  KeyRound,
  Link2,
  Plus,
  RefreshCw,
  Shield,
  Unplug,
  Wallet as WalletIcon,
  X,
} from "lucide-react"
import {
  CreateTonMnemonic,
  formatUnits,
  type ConnectionRequestEvent,
  type RequestErrorEvent,
  type SendTransactionRequestEvent,
  type SignDataRequestEvent,
  type TONConnectSession,
} from "@ton/walletkit"

import {useTonConnectPasteHandler} from "../hooks/useTonConnectPasteHandler"
import {
  addStoredWalletToKit,
  createStoredWallet,
  createWalletKit,
  getWalletNetworkLabel,
} from "../kit"
import {
  loadActiveWalletId,
  loadStoredWallets,
  saveActiveWalletId,
  saveStoredWallets,
} from "../storage"
import type {RuntimeWallet, StoredWallet, WalletNetwork, WalletVersion} from "../types"
import styles from "./WalletPage.module.css"

type WalletPageMode = "overview" | "create" | "import"

interface WalletPageProps {
  readonly host: string
}

const EMPTY_MNEMONIC = Array<string>(24).fill("")
const DEFAULT_WALLET_NETWORK: WalletNetwork = "testnet"
const DEFAULT_WALLET_VERSION: WalletVersion = "v4r2"

export const WalletPage: React.FC<WalletPageProps> = ({host}) => {
  const [storedWallets, setStoredWallets] = useState<StoredWallet[]>(() => loadStoredWallets())
  const [activeWalletId, setActiveWalletId] = useState<string | null>(() => loadActiveWalletId())
  const [mode, setMode] = useState<WalletPageMode>("overview")
  const [walletKit, setWalletKit] = useState<ReturnType<typeof createWalletKit> | null>(null)
  const [runtimeWallets, setRuntimeWallets] = useState<RuntimeWallet[]>([])
  const [sessions, setSessions] = useState<TONConnectSession[]>([])
  const [isInitializing, setIsInitializing] = useState(true)
  const [isSyncingWallets, setIsSyncingWallets] = useState(false)
  const [isSubmitting, setIsSubmitting] = useState(false)
  const [isRefreshingBalance, setIsRefreshingBalance] = useState(false)
  const [balance, setBalance] = useState<string | null>(null)
  const [lastBalanceRefreshAt, setLastBalanceRefreshAt] = useState<number | null>(null)
  const [statusMessage, setStatusMessage] = useState<string | null>(null)
  const [errorMessage, setErrorMessage] = useState<string | null>(null)
  const [copiedAddress, setCopiedAddress] = useState<string | null>(null)
  const [tonConnectUrl, setTonConnectUrl] = useState("")
  const [generatedMnemonic, setGeneratedMnemonic] = useState<string[]>([])
  const [isGeneratingMnemonic, setIsGeneratingMnemonic] = useState(false)
  const [isMnemonicVisible, setIsMnemonicVisible] = useState(false)
  const [isMnemonicSaved, setIsMnemonicSaved] = useState(false)
  const [importMnemonic, setImportMnemonic] = useState("")
  const [selectedConnectWalletId, setSelectedConnectWalletId] = useState<string | null>(null)
  const [pendingConnectRequest, setPendingConnectRequest] = useState<ConnectionRequestEvent | null>(
    null,
  )
  const [pendingTransactionRequest, setPendingTransactionRequest] =
    useState<SendTransactionRequestEvent | null>(null)
  const [pendingSignDataRequest, setPendingSignDataRequest] = useState<SignDataRequestEvent | null>(
    null,
  )

  const activeWallet = useMemo(
    () => runtimeWallets.find(wallet => wallet.record.id === activeWalletId) ?? null,
    [activeWalletId, runtimeWallets],
  )

  const visibleCreateMnemonic = generatedMnemonic.length > 0 ? generatedMnemonic : EMPTY_MNEMONIC
  const importedWordCount = useMemo(
    () => normalizeMnemonic(importMnemonic).length,
    [importMnemonic],
  )
  const selectedConnectWallet =
    runtimeWallets.find(wallet => wallet.record.id === selectedConnectWalletId) ?? activeWallet

  const updateStoredWallets = React.useCallback((nextWallets: StoredWallet[]) => {
    saveStoredWallets(nextWallets)
    setStoredWallets(nextWallets)
  }, [])

  const updateActiveWalletId = React.useCallback((walletId: string | null) => {
    saveActiveWalletId(walletId)
    setActiveWalletId(walletId)
  }, [])

  const refreshSessions = React.useCallback(
    async (kit = walletKit) => {
      if (!kit) {
        return
      }

      const nextSessions = await kit.listSessions()
      setSessions(nextSessions)
    },
    [walletKit],
  )

  const refreshBalance = React.useCallback(async () => {
    if (!activeWallet) {
      setBalance(null)
      setLastBalanceRefreshAt(null)
      return
    }

    setIsRefreshingBalance(true)
    setErrorMessage(null)
    try {
      const nextBalance = await activeWallet.wallet.getBalance()
      setBalance(nextBalance)
      setLastBalanceRefreshAt(Date.now())
    } catch (error) {
      setErrorMessage(getErrorMessage(error, "Failed to refresh wallet balance."))
    } finally {
      setIsRefreshingBalance(false)
    }
  }, [activeWallet])

  useEffect(() => {
    let isCancelled = false
    const nextWalletKit = createWalletKit(host)

    const handleRequestError = (event: RequestErrorEvent) => {
      const fallback = "WalletKit request failed"
      const nextMessage =
        typeof event.error?.message === "string" && event.error.message.length > 0
          ? event.error.message
          : fallback
      setErrorMessage(nextMessage)
    }

    const initialize = async () => {
      try {
        await nextWalletKit.ensureInitialized()

        if (isCancelled) {
          await nextWalletKit.close()
          return
        }

        nextWalletKit.onConnectRequest(event => setPendingConnectRequest(event))
        nextWalletKit.onTransactionRequest(event => setPendingTransactionRequest(event))
        nextWalletKit.onSignDataRequest(event => setPendingSignDataRequest(event))
        nextWalletKit.onDisconnect(() => {
          setStatusMessage("Session disconnected.")
          void nextWalletKit
            .listSessions()
            .then(setSessions)
            .catch(() => {})
        })
        nextWalletKit.onRequestError(handleRequestError)

        setWalletKit(nextWalletKit)
        setSessions(await nextWalletKit.listSessions())
      } catch (error) {
        if (!isCancelled) {
          setErrorMessage(getErrorMessage(error, "Failed to initialize wallet runtime."))
        }
      } finally {
        if (!isCancelled) {
          setIsInitializing(false)
        }
      }
    }

    void initialize()

    return () => {
      isCancelled = true
      void nextWalletKit.close()
    }
  }, [host])

  useEffect(() => {
    if (!walletKit) {
      return
    }

    let isCancelled = false

    const syncWallets = async () => {
      setIsSyncingWallets(true)

      try {
        await walletKit.clearWallets()

        const nextRuntimeWallets: RuntimeWallet[] = []
        for (const walletRecord of storedWallets) {
          const wallet = await addStoredWalletToKit(walletKit, walletRecord)
          if (wallet) {
            nextRuntimeWallets.push({record: walletRecord, wallet})
          }
        }

        if (!isCancelled) {
          setRuntimeWallets(nextRuntimeWallets)
          await refreshSessions(walletKit)
        }
      } catch (error) {
        if (!isCancelled) {
          setErrorMessage(getErrorMessage(error, "Failed to load local wallets into WalletKit."))
        }
      } finally {
        if (!isCancelled) {
          setIsSyncingWallets(false)
        }
      }
    }

    void syncWallets()

    return () => {
      isCancelled = true
    }
  }, [walletKit, storedWallets, refreshSessions])

  useEffect(() => {
    if (runtimeWallets.length === 0) {
      if (activeWalletId !== null) {
        updateActiveWalletId(null)
      }
      return
    }

    const hasActiveWallet = activeWalletId
      ? runtimeWallets.some(wallet => wallet.record.id === activeWalletId)
      : false

    if (!hasActiveWallet) {
      updateActiveWalletId(runtimeWallets[0].record.id)
    }
  }, [activeWalletId, runtimeWallets, updateActiveWalletId])

  useEffect(() => {
    if (!pendingConnectRequest) {
      setSelectedConnectWalletId(activeWallet?.record.id ?? null)
      return
    }

    setSelectedConnectWalletId(
      prev => prev ?? activeWallet?.record.id ?? runtimeWallets[0]?.record.id ?? null,
    )
  }, [activeWallet, pendingConnectRequest, runtimeWallets])

  useEffect(() => {
    if (!statusMessage) {
      return
    }

    const timeoutId = window.setTimeout(() => {
      setStatusMessage(null)
    }, 4500)

    return () => window.clearTimeout(timeoutId)
  }, [statusMessage])

  useEffect(() => {
    if (!copiedAddress) {
      return
    }

    const timeoutId = window.setTimeout(() => {
      setCopiedAddress(null)
    }, 2000)

    return () => window.clearTimeout(timeoutId)
  }, [copiedAddress])

  useEffect(() => {
    void refreshBalance()
  }, [refreshBalance])

  useEffect(() => {
    if (mode === "create" && generatedMnemonic.length === 0) {
      void handleGenerateMnemonic()
    }
  }, [generatedMnemonic.length, mode])

  const handleGenerateMnemonic = React.useCallback(async () => {
    setIsGeneratingMnemonic(true)
    setErrorMessage(null)

    try {
      const nextMnemonic = await CreateTonMnemonic()
      setGeneratedMnemonic(nextMnemonic)
      setIsMnemonicVisible(false)
      setIsMnemonicSaved(false)
    } catch (error) {
      setErrorMessage(getErrorMessage(error, "Failed to generate mnemonic phrase."))
    } finally {
      setIsGeneratingMnemonic(false)
    }
  }, [])

  const handleCreateWallet = async () => {
    if (!walletKit || generatedMnemonic.length === 0) {
      return
    }

    setIsSubmitting(true)
    setErrorMessage(null)

    try {
      const walletRecord = await createStoredWallet(walletKit, {
        mnemonic: generatedMnemonic,
        name: createWalletName(storedWallets),
        network: DEFAULT_WALLET_NETWORK,
        version: DEFAULT_WALLET_VERSION,
      })

      ensureWalletIsUnique(storedWallets, walletRecord)

      const nextWallets = [walletRecord, ...storedWallets]
      updateStoredWallets(nextWallets)
      updateActiveWalletId(walletRecord.id)
      setStatusMessage(`${walletRecord.name} created locally.`)
      setMode("overview")
      setGeneratedMnemonic([])
      setIsMnemonicVisible(false)
      setIsMnemonicSaved(false)
    } catch (error) {
      setErrorMessage(getErrorMessage(error, "Failed to create wallet."))
    } finally {
      setIsSubmitting(false)
    }
  }

  const handleImportWallet = async () => {
    if (!walletKit) {
      return
    }

    setIsSubmitting(true)
    setErrorMessage(null)

    try {
      const mnemonic = normalizeMnemonic(importMnemonic)
      validateMnemonic(mnemonic)

      const walletRecord = await createStoredWallet(walletKit, {
        mnemonic,
        name: createWalletName(storedWallets),
        network: DEFAULT_WALLET_NETWORK,
        version: DEFAULT_WALLET_VERSION,
      })

      ensureWalletIsUnique(storedWallets, walletRecord)

      const nextWallets = [walletRecord, ...storedWallets]
      updateStoredWallets(nextWallets)
      updateActiveWalletId(walletRecord.id)
      setImportMnemonic("")
      setMode("overview")
      setStatusMessage(`${walletRecord.name} imported locally.`)
    } catch (error) {
      setErrorMessage(getErrorMessage(error, "Failed to import wallet."))
    } finally {
      setIsSubmitting(false)
    }
  }

  const handleRemoveWallet = async (walletRecord: StoredWallet) => {
    if (!walletKit) {
      return
    }

    const shouldRemove = window.confirm(`Remove ${walletRecord.name} from this browser?`)
    if (!shouldRemove) {
      return
    }

    setErrorMessage(null)

    try {
      const walletSessions = sessions.filter(session => session.walletId === walletRecord.id)
      await Promise.all(walletSessions.map(session => walletKit.disconnect(session.sessionId)))

      const nextWallets = storedWallets.filter(wallet => wallet.id !== walletRecord.id)
      updateStoredWallets(nextWallets)

      if (activeWalletId === walletRecord.id) {
        updateActiveWalletId(null)
      }

      setStatusMessage(`${walletRecord.name} removed from this browser.`)
    } catch (error) {
      setErrorMessage(getErrorMessage(error, "Failed to remove wallet."))
    }
  }

  const handleApproveConnect = async () => {
    if (!walletKit || !pendingConnectRequest || !selectedConnectWallet) {
      return
    }

    setIsSubmitting(true)
    setErrorMessage(null)

    try {
      await walletKit.approveConnectRequest({
        ...pendingConnectRequest,
        walletAddress: selectedConnectWallet.record.address,
        walletId: selectedConnectWallet.record.id,
      })
      setPendingConnectRequest(null)
      setStatusMessage(
        `Connected ${getDappName(pendingConnectRequest.preview.dAppInfo?.name)} to ${selectedConnectWallet.record.name}.`,
      )
      await refreshSessions()
    } catch (error) {
      setErrorMessage(getErrorMessage(error, "Failed to approve connection request."))
    } finally {
      setIsSubmitting(false)
    }
  }

  const handleRejectConnect = async () => {
    if (!walletKit || !pendingConnectRequest) {
      return
    }

    setIsSubmitting(true)
    setErrorMessage(null)

    try {
      await walletKit.rejectConnectRequest(pendingConnectRequest, "User rejected the connection")
      setPendingConnectRequest(null)
      setStatusMessage("Connection request rejected.")
    } catch (error) {
      setErrorMessage(getErrorMessage(error, "Failed to reject connection request."))
    } finally {
      setIsSubmitting(false)
    }
  }

  const handleApproveTransaction = async () => {
    if (!walletKit || !pendingTransactionRequest) {
      return
    }

    setIsSubmitting(true)
    setErrorMessage(null)

    try {
      await walletKit.approveTransactionRequest(pendingTransactionRequest)
      setPendingTransactionRequest(null)
      setStatusMessage("Transaction request approved.")
    } catch (error) {
      setErrorMessage(getErrorMessage(error, "Failed to approve transaction request."))
    } finally {
      setIsSubmitting(false)
    }
  }

  const handleRejectTransaction = async () => {
    if (!walletKit || !pendingTransactionRequest) {
      return
    }

    setIsSubmitting(true)
    setErrorMessage(null)

    try {
      await walletKit.rejectTransactionRequest(
        pendingTransactionRequest,
        "User rejected the transaction",
      )
      setPendingTransactionRequest(null)
      setStatusMessage("Transaction request rejected.")
    } catch (error) {
      setErrorMessage(getErrorMessage(error, "Failed to reject transaction request."))
    } finally {
      setIsSubmitting(false)
    }
  }

  const handleApproveSignData = async () => {
    if (!walletKit || !pendingSignDataRequest) {
      return
    }

    setIsSubmitting(true)
    setErrorMessage(null)

    try {
      await walletKit.approveSignDataRequest(pendingSignDataRequest)
      setPendingSignDataRequest(null)
      setStatusMessage("Sign request approved.")
    } catch (error) {
      setErrorMessage(getErrorMessage(error, "Failed to approve sign request."))
    } finally {
      setIsSubmitting(false)
    }
  }

  const handleRejectSignData = async () => {
    if (!walletKit || !pendingSignDataRequest) {
      return
    }

    setIsSubmitting(true)
    setErrorMessage(null)

    try {
      await walletKit.rejectSignDataRequest(
        pendingSignDataRequest,
        "User rejected the sign request",
      )
      setPendingSignDataRequest(null)
      setStatusMessage("Sign request rejected.")
    } catch (error) {
      setErrorMessage(getErrorMessage(error, "Failed to reject sign request."))
    } finally {
      setIsSubmitting(false)
    }
  }

  const handleConnectUrlSubmit = async (event: React.FormEvent) => {
    event.preventDefault()

    if (!walletKit || tonConnectUrl.trim().length === 0) {
      return
    }

    setIsSubmitting(true)
    setErrorMessage(null)

    try {
      await walletKit.handleTonConnectUrl(tonConnectUrl.trim())
      setTonConnectUrl("")
      setStatusMessage("TON Connect request received.")
    } catch (error) {
      setErrorMessage(getErrorMessage(error, "Failed to process TON Connect URL."))
    } finally {
      setIsSubmitting(false)
    }
  }

  const handleTonConnectPaste = React.useCallback(
    async (url: string) => {
      if (!walletKit || runtimeWallets.length === 0) {
        return
      }

      setErrorMessage(null)

      try {
        await walletKit.handleTonConnectUrl(url)
        setTonConnectUrl("")
        setStatusMessage("TON Connect request received.")
      } catch (error) {
        setErrorMessage(getErrorMessage(error, "Failed to process TON Connect URL."))
      }
    },
    [runtimeWallets.length, walletKit],
  )

  useTonConnectPasteHandler(handleTonConnectPaste)

  const handleDisconnectSession = async (sessionId: string) => {
    if (!walletKit) {
      return
    }

    setIsSubmitting(true)
    setErrorMessage(null)

    try {
      await walletKit.disconnect(sessionId)
      await refreshSessions()
      setStatusMessage("Session disconnected.")
    } catch (error) {
      setErrorMessage(getErrorMessage(error, "Failed to disconnect session."))
    } finally {
      setIsSubmitting(false)
    }
  }

  const handleCopyAddress = React.useCallback(async (address: string) => {
    try {
      await navigator.clipboard.writeText(address)
      setCopiedAddress(address)
      setErrorMessage(null)
    } catch (error) {
      setErrorMessage(getErrorMessage(error, "Failed to copy address."))
    }
  }, [])

  const pendingRequestCount =
    Number(Boolean(pendingConnectRequest)) +
    Number(Boolean(pendingTransactionRequest)) +
    Number(Boolean(pendingSignDataRequest))
  const hostLabel = formatHostLabel(host)
  const pendingApprovalsText =
    pendingRequestCount === 0
      ? "No pending approvals."
      : `${pendingRequestCount} pending approval${pendingRequestCount === 1 ? "" : "s"}.`
  const connectionsDescription = `Handles TON Connect links on ${hostLabel}. ${pendingApprovalsText}`
  const balanceStatusText = !activeWallet
    ? "Select a wallet to load a balance."
    : isRefreshingBalance
      ? "Updating balance..."
      : lastBalanceRefreshAt
        ? `Updated ${formatRelativeRefreshTime(lastBalanceRefreshAt)}`
        : "Balance not loaded yet."

  return (
    <div className={styles.page}>
      <div className={styles.pageHeader}>
        <div className={styles.headerText}>
          <h1 className={styles.pageTitle}>Wallets</h1>
          <p className={styles.pageDescription}>
            Create browser wallets, route TON Connect sessions, and review signing or transaction
            requests from the same explorer surface.
          </p>
        </div>

        <div className={styles.pageActions}>
          <button
            type="button"
            className={`${styles.secondaryButton} ${
              mode === "create" ? styles.secondaryButtonActive : ""
            }`}
            onClick={() => setMode(mode === "create" ? "overview" : "create")}
          >
            <Plus size={16} />
            {mode === "create" ? "Hide create" : "Create wallet"}
          </button>
          <button
            type="button"
            className={`${styles.secondaryButton} ${
              mode === "import" ? styles.secondaryButtonActive : ""
            }`}
            onClick={() => setMode(mode === "import" ? "overview" : "import")}
          >
            <Import size={16} />
            {mode === "import" ? "Hide import" : "Import wallet"}
          </button>
        </div>
      </div>

      {statusMessage ? (
        <div className={styles.noticeSuccess} role="status">
          <span>{statusMessage}</span>
          <button
            type="button"
            className={styles.noticeCloseButton}
            onClick={() => setStatusMessage(null)}
            aria-label="Dismiss message"
          >
            <X size={14} />
          </button>
        </div>
      ) : null}
      {errorMessage ? (
        <div className={styles.noticeError} role="alert">
          <span>{errorMessage}</span>
          <button
            type="button"
            className={styles.noticeCloseButton}
            onClick={() => setErrorMessage(null)}
            aria-label="Dismiss error"
          >
            <X size={14} />
          </button>
        </div>
      ) : null}

      <section className={styles.layout}>
        <div className={styles.mainColumn}>
          <section className={styles.card}>
            <div className={styles.cardHeader}>
              <div className={styles.headerText}>
                <h2 className={styles.cardTitle}>Wallets</h2>
                <p className={styles.cardDescription}>
                  Mnemonics are stored only in this browser for now. This is suitable for local
                  development, not for production custody.
                </p>
              </div>
            </div>

            {isInitializing || isSyncingWallets ? (
              <div className={styles.loadingState}>Loading wallet runtime...</div>
            ) : runtimeWallets.length === 0 ? (
              <div className={`${styles.emptyState} ${styles.emptyStateProminent}`}>
                <div className={styles.emptyStateLead}>
                  <span className={styles.walletGlyph}>
                    <WalletIcon size={18} />
                  </span>
                  <div>
                    <div className={styles.emptyStateTitle}>No local wallets yet</div>
                    <div className={styles.emptyStateDescription}>
                      Create a browser wallet or import a mnemonic to start approving local dApp
                      requests.
                    </div>
                  </div>
                </div>

                <div className={styles.emptyStateActions}>
                  <button
                    type="button"
                    className={styles.secondaryButton}
                    onClick={() => setMode("create")}
                  >
                    <Plus size={16} />
                    Create wallet
                  </button>
                  <button
                    type="button"
                    className={styles.secondaryButton}
                    onClick={() => setMode("import")}
                  >
                    <Import size={16} />
                    Import mnemonic
                  </button>
                </div>
              </div>
            ) : (
              <div className={styles.walletList}>
                {runtimeWallets.map(wallet => {
                  const isActive = wallet.record.id === activeWalletId
                  return (
                    <article
                      key={wallet.record.id}
                      className={`${styles.walletCard} ${isActive ? styles.walletCardActive : ""}`}
                    >
                      <button
                        type="button"
                        className={styles.walletSelect}
                        onClick={() => updateActiveWalletId(wallet.record.id)}
                      >
                        <div className={styles.walletLead}>
                          <span className={styles.walletGlyph}>
                            <WalletIcon size={16} />
                          </span>

                          <div className={styles.walletNameRow}>
                            <div className={styles.walletIdentity}>
                              <span className={styles.walletName}>{wallet.record.name}</span>
                              <span className={styles.walletSubtitle}>
                                {isActive ? "Active wallet" : "Local browser wallet"} ·{" "}
                                {getWalletNetworkLabel(wallet.record.network)} ·{" "}
                                {wallet.record.version.toUpperCase()}
                              </span>
                            </div>
                          </div>
                        </div>

                        <span className={styles.radio}>
                          {isActive ? <Check size={14} /> : null}
                        </span>
                      </button>

                      <CopyableAddress
                        address={wallet.record.address}
                        copiedAddress={copiedAddress}
                        onCopy={handleCopyAddress}
                        visibleChars={18}
                      />

                      <div className={styles.walletMetaRow}>
                        <Link
                          to={`/explorer/address/${wallet.record.address}`}
                          className={styles.actionButton}
                        >
                          Open in Explorer
                          <ArrowUpRight size={14} />
                        </Link>
                        <button
                          type="button"
                          className={`${styles.actionButton} ${styles.removeButton}`}
                          onClick={() => void handleRemoveWallet(wallet.record)}
                        >
                          Remove
                        </button>
                      </div>
                    </article>
                  )
                })}
              </div>
            )}
          </section>

          {mode === "create" ? (
            <section className={`${styles.card} ${styles.editorCard}`}>
              <div className={styles.cardHeader}>
                <div className={styles.headerText}>
                  <h2 className={styles.cardTitle}>Create Wallet</h2>
                  <p className={styles.cardDescription}>
                    Generate a new testnet V4R2 wallet locally in the browser and keep the recovery
                    phrase on this device only.
                  </p>
                </div>
                <button
                  type="button"
                  className={styles.inlineButton}
                  onClick={() => setMode("overview")}
                >
                  Close
                </button>
              </div>

              <div className={styles.mnemonicPanel}>
                <div className={styles.mnemonicHeader}>
                  <span>Recovery phrase</span>
                  <button
                    type="button"
                    className={styles.inlineButton}
                    onClick={() => void handleGenerateMnemonic()}
                    disabled={isGeneratingMnemonic}
                  >
                    <RefreshCw size={14} className={isGeneratingMnemonic ? styles.spinning : ""} />
                    Generate new
                  </button>
                </div>

                <div
                  className={`${styles.mnemonicGrid} ${
                    !isMnemonicVisible ? styles.mnemonicGridHidden : ""
                  }`}
                >
                  {visibleCreateMnemonic.map((word, index) => (
                    <div key={`${word}-${index}`} className={styles.mnemonicWord}>
                      <span className={styles.wordIndex}>{index + 1}</span>
                      <span>{word || "••••••"}</span>
                    </div>
                  ))}
                </div>

                {!isMnemonicVisible && generatedMnemonic.length > 0 ? (
                  <button
                    type="button"
                    className={styles.primaryButton}
                    onClick={() => setIsMnemonicVisible(true)}
                  >
                    Reveal phrase
                  </button>
                ) : null}

                {isMnemonicVisible ? (
                  <label className={styles.checkboxRow}>
                    <input
                      type="checkbox"
                      checked={isMnemonicSaved}
                      onChange={event => setIsMnemonicSaved(event.target.checked)}
                    />
                    <span>I have saved the recovery phrase.</span>
                  </label>
                ) : null}

                <button
                  type="button"
                  className={styles.primaryButton}
                  onClick={() => void handleCreateWallet()}
                  disabled={!isMnemonicVisible || !isMnemonicSaved || isSubmitting}
                >
                  Create wallet
                </button>
              </div>

              <p className={styles.helperText}>New wallets are created as Testnet V4R2.</p>
            </section>
          ) : null}

          {mode === "import" ? (
            <section className={`${styles.card} ${styles.editorCard}`}>
              <div className={styles.cardHeader}>
                <div className={styles.headerText}>
                  <h2 className={styles.cardTitle}>Import Wallet</h2>
                  <p className={styles.cardDescription}>
                    Paste a 12 or 24-word TON mnemonic phrase and recreate the testnet V4R2 wallet
                    locally.
                  </p>
                </div>
                <button
                  type="button"
                  className={styles.inlineButton}
                  onClick={() => setMode("overview")}
                >
                  Close
                </button>
              </div>

              <label className={styles.fieldLabel} htmlFor="wallet-import">
                Mnemonic phrase
              </label>
              <textarea
                id="wallet-import"
                value={importMnemonic}
                onChange={event => setImportMnemonic(event.target.value)}
                className={styles.textArea}
                rows={5}
                placeholder="word1 word2 word3 ..."
              />
              <div className={styles.importMeta}>
                <span>{importedWordCount}/24 words</span>
                <button
                  type="button"
                  className={styles.inlineButton}
                  onClick={async () => {
                    const clipboardText = await navigator.clipboard.readText()
                    setImportMnemonic(clipboardText)
                  }}
                >
                  Paste from clipboard
                </button>
              </div>

              <button
                type="button"
                className={styles.primaryButton}
                onClick={() => void handleImportWallet()}
                disabled={importedWordCount < 12 || isSubmitting}
              >
                Import wallet
              </button>

              <p className={styles.helperText}>Imported wallets are opened as Testnet V4R2.</p>
            </section>
          ) : null}
        </div>

        <div className={styles.sideColumn}>
          <section className={styles.card}>
            <div className={styles.cardHeader}>
              <div className={styles.headerText}>
                <h2 className={styles.cardTitle}>Active Wallet</h2>
                <p className={styles.cardDescription}>
                  Use this wallet for balances, TON Connect sessions, and request approvals.
                </p>
              </div>
              <button
                type="button"
                className={`${styles.inlineButton} ${styles.refreshButton}`}
                onClick={() => void refreshBalance()}
                disabled={!activeWallet || isRefreshingBalance}
                aria-live="polite"
              >
                <RefreshCw size={14} className={isRefreshingBalance ? styles.spinning : ""} />
                Refresh
              </button>
            </div>

            {activeWallet ? (
              <div className={styles.activeWalletContent}>
                <div className={styles.activeWalletHeader}>
                  <div>
                    <div className={styles.activeWalletName}>{activeWallet.record.name}</div>
                    <div className={styles.activeWalletSubhead}>
                      Local wallet · {getWalletNetworkLabel(activeWallet.record.network)} ·{" "}
                      {activeWallet.record.version.toUpperCase()}
                    </div>
                  </div>
                </div>

                <div className={styles.balanceBlock}>
                  <span className={styles.balanceLabel}>Balance</span>
                  <span className={styles.balanceValue}>
                    {balance ? `${formatCompactTonBalance(balance)} TON` : "Loading..."}
                  </span>
                  <span className={styles.balanceMeta} aria-live="polite">
                    {balanceStatusText}
                  </span>
                </div>

                <CopyableAddress
                  address={activeWallet.record.address}
                  copiedAddress={copiedAddress}
                  onCopy={handleCopyAddress}
                  visibleChars={18}
                />

                <div className={styles.activeWalletActions}>
                  <Link
                    to={`/explorer/address/${activeWallet.record.address}`}
                    className={styles.actionButton}
                  >
                    Open in Explorer
                    <ArrowUpRight size={14} />
                  </Link>
                </div>
              </div>
            ) : (
              <div className={styles.emptyState}>Select or create a wallet to continue.</div>
            )}
          </section>

          <section className={`${styles.card} ${styles.connectionCard}`}>
            <div className={styles.cardHeader}>
              <div className={styles.headerText}>
                <h2 className={styles.cardTitle}>Connections</h2>
                <p className={styles.cardDescription}>{connectionsDescription}</p>
              </div>
            </div>

            <div className={styles.subsection}>
              <div>
                <h3 className={styles.subsectionTitle}>New TON Connect request</h3>
                <p className={styles.subsectionDescription}>
                  Paste a `tonconnect://` link from a local dApp to start a session.
                </p>
              </div>
              <form
                className={styles.connectForm}
                onSubmit={event => void handleConnectUrlSubmit(event)}
              >
                <label className={styles.fieldLabel} htmlFor="ton-connect-url">
                  Connect URL
                </label>
                <textarea
                  id="ton-connect-url"
                  className={styles.textArea}
                  rows={4}
                  value={tonConnectUrl}
                  onChange={event => setTonConnectUrl(event.target.value)}
                  placeholder="tonconnect://..."
                  disabled={runtimeWallets.length === 0 || isSubmitting}
                />
                <button
                  type="submit"
                  className={styles.primaryButton}
                  disabled={
                    runtimeWallets.length === 0 || tonConnectUrl.trim().length === 0 || isSubmitting
                  }
                >
                  <Link2 size={16} />
                  Handle request
                </button>
                <p className={styles.helperText}>
                  You can also paste a connect link anywhere on this page.
                </p>
              </form>
            </div>

            <div className={styles.subsectionDivider} />

            <div className={styles.subsection}>
              <div>
                <h3 className={styles.subsectionTitle}>Active Sessions</h3>
                <p className={styles.subsectionDescription}>
                  Connected dApps currently bound to local wallets in this browser.
                </p>
              </div>

              {sessions.length === 0 ? (
                <div className={styles.emptyState}>No active TON Connect sessions.</div>
              ) : (
                <div className={styles.sessionList}>
                  {sessions.map(session => (
                    <article key={session.sessionId} className={styles.sessionCard}>
                      <div className={styles.sessionHeader}>
                        <div>
                          <div className={styles.sessionTitle}>{getDappName(session.dAppName)}</div>
                          <div className={styles.sessionDomain}>{session.domain}</div>
                        </div>
                        <button
                          type="button"
                          className={styles.inlineDangerButton}
                          onClick={() => void handleDisconnectSession(session.sessionId)}
                        >
                          <Unplug size={14} />
                          Disconnect
                        </button>
                      </div>
                      <div className={styles.sessionMeta}>
                        <MetaRow label="Wallet">
                          {findWalletName(runtimeWallets, session.walletId)}
                        </MetaRow>
                        <MetaRow label="Last activity">
                          {formatDateTime(session.lastActivityAt)}
                        </MetaRow>
                      </div>
                    </article>
                  ))}
                </div>
              )}
            </div>
          </section>
        </div>
      </section>

      {pendingConnectRequest ? (
        <ModalShell
          title="Connection Request"
          subtitle={`${getDappName(pendingConnectRequest.preview.dAppInfo?.name)} wants to connect to a local wallet.`}
        >
          <div className={styles.modalContent}>
            <div className={styles.permissionsList}>
              {pendingConnectRequest.preview.permissions.map((permission, index) => (
                <div key={`${permission.name}-${index}`} className={styles.permissionItem}>
                  <Shield size={15} />
                  <div>
                    <div className={styles.permissionTitle}>
                      {permission.title ?? permission.name ?? "Permission"}
                    </div>
                    <div className={styles.permissionDescription}>
                      {permission.description ?? "Requested by the dApp during connect."}
                    </div>
                  </div>
                </div>
              ))}
            </div>

            <div className={styles.walletPicker}>
              <span className={styles.fieldLabel}>Connect with</span>
              {runtimeWallets.map(wallet => {
                const isSelected = wallet.record.id === selectedConnectWallet?.record.id
                return (
                  <button
                    key={wallet.record.id}
                    type="button"
                    className={`${styles.pickerOption} ${isSelected ? styles.pickerOptionActive : ""}`}
                    onClick={() => setSelectedConnectWalletId(wallet.record.id)}
                  >
                    <div>
                      <div className={styles.pickerTitle}>{wallet.record.name}</div>
                      <div className={styles.pickerSubtitle}>
                        {shortenAddress(wallet.record.address)} ·{" "}
                        {getWalletNetworkLabel(wallet.record.network)}
                      </div>
                    </div>
                    <span className={styles.radio}>{isSelected ? <Check size={14} /> : null}</span>
                  </button>
                )
              })}
            </div>

            <div className={styles.modalActions}>
              <button
                type="button"
                className={styles.secondaryButton}
                onClick={() => void handleRejectConnect()}
                disabled={isSubmitting}
              >
                Reject
              </button>
              <button
                type="button"
                className={styles.primaryButton}
                onClick={() => void handleApproveConnect()}
                disabled={!selectedConnectWallet || isSubmitting}
              >
                Connect
              </button>
            </div>
          </div>
        </ModalShell>
      ) : null}

      {pendingTransactionRequest ? (
        <ModalShell
          title="Transaction Request"
          subtitle={`${getDappName(pendingTransactionRequest.dAppInfo?.name)} wants this wallet to send a transaction.`}
        >
          <div className={styles.modalContent}>
            <div className={styles.requestSummary}>
              <MetaRow label="Messages">
                {String(pendingTransactionRequest.request.messages.length)}
              </MetaRow>
              <MetaRow label="Network">
                {pendingTransactionRequest.request.network?.chainId === "-239"
                  ? "Mainnet"
                  : "Testnet"}
              </MetaRow>
              <MetaRow label="Amount">
                {formatTonBalance(
                  pendingTransactionRequest.request.messages
                    .reduce((sum, message) => {
                      return sum + BigInt(message.amount)
                    }, 0n)
                    .toString(),
                )}{" "}
                TON
              </MetaRow>
            </div>

            <div className={styles.requestMessages}>
              {pendingTransactionRequest.request.messages.map((message, index) => (
                <div key={`${message.address}-${index}`} className={styles.messageItem}>
                  <span className={styles.messageIndex}>#{index + 1}</span>
                  <div>
                    <CopyableAddress
                      address={message.address}
                      copiedAddress={copiedAddress}
                      onCopy={handleCopyAddress}
                      visibleChars={18}
                    />
                    <div className={styles.messageValue}>
                      {formatTonBalance(message.amount)} TON
                    </div>
                  </div>
                </div>
              ))}
            </div>

            <div className={styles.modalActions}>
              <button
                type="button"
                className={styles.secondaryButton}
                onClick={() => void handleRejectTransaction()}
                disabled={isSubmitting}
              >
                Reject
              </button>
              <button
                type="button"
                className={styles.primaryButton}
                onClick={() => void handleApproveTransaction()}
                disabled={isSubmitting}
              >
                Approve
              </button>
            </div>
          </div>
        </ModalShell>
      ) : null}

      {pendingSignDataRequest ? (
        <ModalShell
          title="Sign Request"
          subtitle={`${getDappName(pendingSignDataRequest.preview.dAppInfo?.name)} wants a signature from the active wallet.`}
        >
          <div className={styles.modalContent}>
            <div className={styles.requestMessages}>
              <div className={styles.messageItem}>
                <KeyRound size={16} />
                <div>
                  <div className={styles.messageAddress}>
                    {pendingSignDataRequest.preview.data.type.toUpperCase()}
                  </div>
                  <div className={styles.permissionDescription}>
                    {describeSignPreview(pendingSignDataRequest.preview.data)}
                  </div>
                </div>
              </div>
            </div>

            <div className={styles.modalActions}>
              <button
                type="button"
                className={styles.secondaryButton}
                onClick={() => void handleRejectSignData()}
                disabled={isSubmitting}
              >
                Reject
              </button>
              <button
                type="button"
                className={styles.primaryButton}
                onClick={() => void handleApproveSignData()}
                disabled={isSubmitting}
              >
                Sign
              </button>
            </div>
          </div>
        </ModalShell>
      ) : null}
    </div>
  )
}

interface ModalShellProps {
  readonly title: string
  readonly subtitle: string
  readonly children: React.ReactNode
}

const ModalShell: React.FC<ModalShellProps> = ({title, subtitle, children}) => {
  return (
    <div className={styles.modalBackdrop}>
      <div className={styles.modalCard}>
        <div className={styles.modalHeader}>
          <h3 className={styles.modalTitle}>{title}</h3>
          <p className={styles.modalSubtitle}>{subtitle}</p>
        </div>
        {children}
      </div>
    </div>
  )
}

interface MetaRowProps {
  readonly label: string
  readonly children: React.ReactNode
}

const MetaRow: React.FC<MetaRowProps> = ({label, children}) => {
  return (
    <div className={styles.metaRow}>
      <span className={styles.metaLabel}>{label}</span>
      <span className={styles.metaValue}>{children}</span>
    </div>
  )
}

interface CopyableAddressProps {
  readonly address: string
  readonly copiedAddress: string | null
  readonly onCopy: (address: string) => Promise<void>
  readonly visibleChars?: number
}

const CopyableAddress: React.FC<CopyableAddressProps> = ({
  address,
  copiedAddress,
  onCopy,
  visibleChars = 12,
}) => {
  const isCopied = copiedAddress === address

  return (
    <div className={styles.copyableAddress}>
      <span className={styles.copyableAddressText} title={address}>
        {shortenAddress(address, visibleChars)}
      </span>
      <button
        type="button"
        className={`${styles.addressCopyButton} ${isCopied ? styles.addressCopyButtonCopied : ""}`}
        onClick={() => void onCopy(address)}
        aria-label={isCopied ? "Address copied" : "Copy address"}
      >
        {isCopied ? <Check size={14} /> : <Copy size={14} />}
      </button>
    </div>
  )
}

function normalizeMnemonic(input: string): string[] {
  return input
    .trim()
    .toLowerCase()
    .split(/\s+/)
    .map(word => word.replace(/[^a-z]/g, ""))
    .filter(Boolean)
}

function validateMnemonic(words: string[]): void {
  if (!(words.length === 12 || words.length === 24)) {
    throw new Error("Expected a 12 or 24-word TON mnemonic phrase.")
  }
}

function createWalletName(wallets: StoredWallet[]): string {
  return `Wallet ${wallets.length + 1}`
}

function ensureWalletIsUnique(existingWallets: StoredWallet[], nextWallet: StoredWallet): void {
  if (
    existingWallets.some(
      wallet => wallet.id === nextWallet.id || wallet.address === nextWallet.address,
    )
  ) {
    throw new Error("This wallet is already imported in the browser.")
  }
}

function shortenAddress(address: string, visibleChars = 12): string {
  if (address.length <= visibleChars * 2) {
    return address
  }

  return `${address.slice(0, visibleChars)}...${address.slice(-visibleChars)}`
}

function formatTonBalance(balance: string): string {
  return formatUnits(balance, 9)
}

function formatCompactTonBalance(balance: string): string {
  const numericBalance = Number(formatTonBalance(balance))

  if (!Number.isFinite(numericBalance)) {
    return formatTonBalance(balance)
  }

  if (numericBalance > 0 && numericBalance < 0.0001) {
    return "<0.0001"
  }

  return numericBalance.toLocaleString(undefined, {
    maximumFractionDigits: 4,
  })
}

function formatDateTime(value: string): string {
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) {
    return value
  }

  return date.toLocaleString()
}

function formatRelativeRefreshTime(timestamp: number): string {
  const elapsed = Math.max(0, Date.now() - timestamp)
  const seconds = Math.floor(elapsed / 1000)

  if (seconds < 5) {
    return "just now"
  }

  if (seconds < 60) {
    return `${seconds}s ago`
  }

  const minutes = Math.floor(seconds / 60)
  if (minutes < 60) {
    return `${minutes}m ago`
  }

  const hours = Math.floor(minutes / 60)
  if (hours < 24) {
    return `${hours}h ago`
  }

  return formatDateTime(new Date(timestamp).toISOString())
}

function getDappName(name: string | undefined): string {
  return name && name.trim().length > 0 ? name : "Unknown dApp"
}

function findWalletName(wallets: RuntimeWallet[], walletId: string): string {
  return wallets.find(wallet => wallet.record.id === walletId)?.record.name ?? "Unknown wallet"
}

function getErrorMessage(error: unknown, fallback: string): string {
  return error instanceof Error && error.message.length > 0 ? error.message : fallback
}

function formatHostLabel(host: string): string {
  try {
    return new URL(host).host
  } catch {
    return host
  }
}

function describeSignPreview(preview: SignDataRequestEvent["preview"]["data"]): string {
  switch (preview.type) {
    case "text":
      return preview.value.content
    case "binary":
      return `${preview.value.content.length} base64 chars`
    case "cell":
      return preview.value.schema ?? "TON Cell payload"
    default:
      return "Unknown sign payload"
  }
}
