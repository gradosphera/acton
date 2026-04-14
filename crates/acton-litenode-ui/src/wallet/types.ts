import type {Wallet} from "@ton/walletkit"

export type WalletNetwork = "mainnet" | "testnet"
export type WalletVersion = "v5r1" | "v4r2"

export interface StoredWallet {
  readonly id: string
  readonly name: string
  readonly mnemonic: string[]
  readonly version: WalletVersion
  readonly network: WalletNetwork
  readonly address: string
  readonly publicKey: string
  readonly createdAt: number
}

export interface RuntimeWallet {
  readonly record: StoredWallet
  readonly wallet: Wallet
}
