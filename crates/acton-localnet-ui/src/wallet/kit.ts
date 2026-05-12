import {
  LocalStorageAdapter,
  Network,
  Signer,
  TonWalletKit,
  WalletV4R2Adapter,
  WalletV5R1Adapter,
  createDeviceInfo,
  createWalletManifest,
  type Wallet,
} from "@ton/walletkit"

import type {StoredWallet, WalletNetwork} from "./types"

const TON_CONNECT_BRIDGE_URL = "https://bridge.tonapi.io/bridge"
const TONKEEPER_APP_NAME = "Tonkeeper"
const TONKEEPER_WALLET_NAME = "tonkeeper"

function getWalletOrigin(): string {
  if (typeof window === "undefined") {
    return "http://localhost:3006"
  }

  return window.location.origin
}

function createWalletIconDataUri(): string {
  const svg = `
    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64">
      <rect width="64" height="64" rx="16" fill="#0088cc"/>
      <path d="M32 10 12 49h40L32 10Zm0 10 10.8 19H21.2L32 20Z" fill="#fff"/>
    </svg>
  `.trim()

  return `data:image/svg+xml;utf8,${encodeURIComponent(svg)}`
}

export function toWalletNetwork(network: WalletNetwork): Network {
  return network === "mainnet" ? Network.mainnet() : Network.testnet()
}

export function getWalletNetworkLabel(network: WalletNetwork): string {
  return network === "mainnet" ? "Mainnet" : "Testnet"
}

export function createWalletKit(baseUrl: string): TonWalletKit {
  const origin = getWalletOrigin()
  const walletUrl = `${origin}/wallet`

  return new TonWalletKit({
    deviceInfo: createDeviceInfo({
      appName: TONKEEPER_APP_NAME,
      appVersion: "0.1.0",
      features: [
        "SendTransaction",
        {name: "SendTransaction", maxMessages: 4},
        {name: "SignData", types: ["text", "binary", "cell"]},
      ],
    }),
    walletManifest: createWalletManifest({
      name: TONKEEPER_WALLET_NAME,
      appName: TONKEEPER_APP_NAME,
      imageUrl: createWalletIconDataUri(),
      aboutUrl: walletUrl,
      universalLink: walletUrl,
      bridgeUrl: TON_CONNECT_BRIDGE_URL,
      jsBridgeKey: TONKEEPER_WALLET_NAME,
      tondns: "tonkeeper.ton",
      injected: false,
      embedded: false,
      platforms: ["chrome", "firefox", "safari", "android", "ios", "windows", "macos", "linux"],
    }),
    networks: {
      [Network.mainnet().chainId]: {
        apiClient: {
          url: "http://localhost:5411/",
        },
      },
      [Network.testnet().chainId]: {
        apiClient: {
          url: "http://localhost:5411/",
        },
      },
    },
    storage: new LocalStorageAdapter({prefix: "acton-walletkit:"}),
    dev: {
      disableManifestDomainCheck: true,
    },
  })
}

export async function createStoredWallet(
  kit: TonWalletKit,
  options: {
    readonly mnemonic: string[]
    readonly name: string
    readonly network: WalletNetwork
    readonly version: StoredWallet["version"]
  },
): Promise<StoredWallet> {
  const signer = await Signer.fromMnemonic(options.mnemonic)
  const network = toWalletNetwork(options.network)
  const client = kit.getApiClient(network)

  const adapter =
    options.version === "v4r2"
      ? await WalletV4R2Adapter.create(signer, {client, network})
      : await WalletV5R1Adapter.create(signer, {client, network})

  return {
    id: adapter.getWalletId(),
    name: options.name,
    mnemonic: options.mnemonic,
    version: options.version,
    network: options.network,
    address: adapter.getAddress({testnet: options.network === "testnet"}),
    publicKey: adapter.getPublicKey(),
    createdAt: Date.now(),
  }
}

export async function addStoredWalletToKit(
  kit: TonWalletKit,
  walletRecord: StoredWallet,
): Promise<Wallet | undefined> {
  const signer = await Signer.fromMnemonic(walletRecord.mnemonic)
  const network = toWalletNetwork(walletRecord.network)
  const client = kit.getApiClient(network)

  const adapter =
    walletRecord.version === "v4r2"
      ? await WalletV4R2Adapter.create(signer, {client, network})
      : await WalletV5R1Adapter.create(signer, {client, network})

  return kit.addWallet(adapter)
}
