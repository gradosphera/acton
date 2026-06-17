import React, {useEffect, useState} from "react"

import type {TonClient} from "../api/client"
import type {JettonMasterMetadata, JettonWallet} from "../api/types"

import styles from "./Tokens.module.css"
import {toRawAddress} from "./utils"

const TOKEN_PLACEHOLDER_IMAGE = "/token-placeholder.svg"

interface TokensProps {
  readonly wallets: JettonWallet[]
  readonly client: TonClient
  readonly onAddressClick?: (addr: string) => void
}

export const Tokens: React.FC<TokensProps> = ({wallets, client, onAddressClick}) => {
  const [mastersByAddress, setMastersByAddress] = useState<Map<string, JettonMasterMetadata>>(
    () => new Map(),
  )

  useEffect(() => {
    let isActive = true

    const fetchMasters = async () => {
      const inlineMasters = new Map<string, JettonMasterMetadata>()
      const missingMasterAddresses = new Set<string>()

      for (const wallet of wallets) {
        const key = toRawAddress(wallet.jetton)
        if (wallet.master) {
          inlineMasters.set(key, wallet.master)
        } else {
          missingMasterAddresses.add(wallet.jetton)
        }
      }

      setMastersByAddress(inlineMasters)
      if (missingMasterAddresses.size === 0) {
        return
      }

      try {
        const masters = await client.getJettonMasters([...missingMasterAddresses])
        if (!isActive) return
        setMastersByAddress(
          new Map([
            ...inlineMasters,
            ...masters.map(master => [toRawAddress(master.address), master] as const),
          ]),
        )
      } catch (error) {
        console.error("Failed to fetch jetton masters", error)
      }
    }

    void fetchMasters()
    return () => {
      isActive = false
    }
  }, [wallets, client])

  if (wallets.length === 0) {
    return <div className={styles.empty}>No tokens found.</div>
  }

  return (
    <div className={styles.container}>
      <div className={styles.list}>
        {wallets.map(w => {
          const master = w.master ?? mastersByAddress.get(toRawAddress(w.jetton))
          const decimals = Number(master?.jetton_content?.decimals || 9)
          const rawBalance = Number(w.balance)
          const rawSupply = Number(master?.total_supply || "0")
          const balance = rawBalance / Math.pow(10, decimals)
          const supplyShare = rawSupply > 0 ? rawBalance / rawSupply : undefined
          const supplyShareLabel =
            supplyShare === undefined
              ? "Unknown"
              : supplyShare === 0
                ? "0%"
                : supplyShare < 0.0001
                  ? "<0.01%"
                  : `${(supplyShare * 100).toLocaleString(undefined, {
                      maximumFractionDigits: supplyShare < 0.01 ? 2 : 1,
                    })}%`
          const symbol = master?.jetton_content?.symbol || "UNKNOWN"

          return (
            <div
              key={w.address}
              className={styles.walletItem}
              onClick={() => onAddressClick?.(w.jetton)}
              onKeyDown={e => {
                if (e.key === "Enter" || e.key === " ") {
                  onAddressClick?.(w.jetton)
                }
              }}
              role="button"
              tabIndex={0}
            >
              <img
                src={master?.jetton_content?.image || TOKEN_PLACEHOLDER_IMAGE}
                alt={symbol}
                className={styles.jettonImage}
                onError={e => {
                  const img = e.currentTarget
                  if (img.getAttribute("src") === TOKEN_PLACEHOLDER_IMAGE) return
                  img.src = TOKEN_PLACEHOLDER_IMAGE
                }}
              />
              <div className={styles.jettonInfoMain}>
                <div className={styles.jettonName}>
                  {master?.jetton_content?.name || "Unknown Jetton"}
                </div>
                <div className={styles.jettonBalanceRow}>
                  <span className={styles.balanceValue}>
                    {balance.toLocaleString(undefined, {
                      maximumFractionDigits: decimals,
                    })}
                  </span>
                  <span className={styles.jettonSymbol}>{symbol}</span>
                </div>
              </div>
              <div className={styles.supplyInfo}>
                <div className={styles.supplyShareValue}>{supplyShareLabel}</div>
                <div className={styles.supplyShareLabel}>of supply</div>
              </div>
            </div>
          )
        })}
      </div>
    </div>
  )
}

export const TokensSkeleton: React.FC = () => {
  return (
    <div className={styles.container} aria-label="Loading tokens">
      <div className={styles.list}>
        {Array.from({length: 5}, (_, index) => (
          <div key={`token-skeleton-${index}`} className={styles.walletItemSkeleton}>
            <div className={`${styles.skeleton} ${styles.jettonImageSkeleton}`} />
            <div className={styles.jettonInfoMain}>
              <div className={`${styles.skeleton} ${styles.jettonNameSkeleton}`} />
              <div className={`${styles.skeleton} ${styles.jettonBalanceSkeleton}`} />
            </div>
            <div className={styles.supplyInfo}>
              <div className={`${styles.skeleton} ${styles.supplyValueSkeleton}`} />
              <div className={`${styles.skeleton} ${styles.supplyLabelSkeleton}`} />
            </div>
          </div>
        ))}
      </div>
    </div>
  )
}
