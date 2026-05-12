import type {StoredWallet} from "./types"

const WALLETS_STORAGE_KEY = "acton.wallets"
const ACTIVE_WALLET_STORAGE_KEY = "acton.wallet.active"

function isStoredWallet(value: unknown): value is StoredWallet {
  if (!value || typeof value !== "object") {
    return false
  }

  const wallet = value as Record<string, unknown>

  return (
    typeof wallet.id === "string" &&
    typeof wallet.name === "string" &&
    Array.isArray(wallet.mnemonic) &&
    wallet.mnemonic.every(word => typeof word === "string") &&
    (wallet.version === "v5r1" || wallet.version === "v4r2") &&
    (wallet.network === "mainnet" || wallet.network === "testnet") &&
    typeof wallet.address === "string" &&
    typeof wallet.publicKey === "string" &&
    typeof wallet.createdAt === "number"
  )
}

export function loadStoredWallets(): StoredWallet[] {
  if (typeof window === "undefined") {
    return []
  }

  try {
    const raw = window.localStorage.getItem(WALLETS_STORAGE_KEY)
    if (!raw) {
      return []
    }

    const parsed = JSON.parse(raw)
    if (!Array.isArray(parsed)) {
      return []
    }

    return parsed.filter(isStoredWallet)
  } catch {
    return []
  }
}

export function saveStoredWallets(wallets: StoredWallet[]): void {
  if (typeof window === "undefined") {
    return
  }

  window.localStorage.setItem(WALLETS_STORAGE_KEY, JSON.stringify(wallets))
}

export function loadActiveWalletId(): string | null {
  if (typeof window === "undefined") {
    return null
  }

  return window.localStorage.getItem(ACTIVE_WALLET_STORAGE_KEY)
}

export function saveActiveWalletId(walletId: string | null): void {
  if (typeof window === "undefined") {
    return
  }

  if (walletId) {
    window.localStorage.setItem(ACTIVE_WALLET_STORAGE_KEY, walletId)
  } else {
    window.localStorage.removeItem(ACTIVE_WALLET_STORAGE_KEY)
  }
}
