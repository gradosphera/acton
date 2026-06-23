import {useEffect, useMemo, useState} from "react"
import type {FC, ReactNode} from "react"

import type {TonClient} from "../api/client"
import type {LocalnetNodeInfo} from "../api/types"

import {
  NetworkInfoContext,
  type ExplorerNetworkInfo,
  type NetworkInfoContextValue,
} from "./useNetworkInfo"

interface NetworkInfoProviderProps {
  readonly client: TonClient
  readonly children: ReactNode
}

export const NetworkInfoProvider: FC<NetworkInfoProviderProps> = ({client, children}) => {
  const [nodeInfo, setNodeInfo] = useState<LocalnetNodeInfo | undefined>()

  useEffect(() => {
    let cancelled = false

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
      }
    }

    void loadNodeInfo()

    return () => {
      cancelled = true
    }
  }, [client])

  const forkNetwork = nodeInfo?.fork_network?.trim()
  const normalizedForkNetwork = forkNetwork?.toLocaleLowerCase()
  const isFork = nodeInfo?.state_source === "remote" && Boolean(forkNetwork)
  const isMainnetFork = isFork && normalizedForkNetwork === "mainnet"
  const network = useMemo<ExplorerNetworkInfo>(() => {
    if (!isFork) {
      return {
        id: "localnet",
        label: "Localnet",
        testOnly: true,
        supportsActions: false,
      }
    }
    if (normalizedForkNetwork === "mainnet") {
      return {
        id: "mainnet",
        label: "Mainnet",
        testOnly: false,
        supportsActions: true,
      }
    }
    if (normalizedForkNetwork === "testnet") {
      return {
        id: "testnet",
        label: "Testnet",
        testOnly: true,
        supportsActions: true,
      }
    }
    return {
      id: `custom:${normalizedForkNetwork ?? "fork"}`,
      label: forkNetwork ?? "Custom",
      testOnly: true,
      supportsActions: false,
    }
  }, [forkNetwork, isFork, normalizedForkNetwork])
  const addressFormat = useMemo(
    () => ({
      testOnly: network.testOnly,
    }),
    [network.testOnly],
  )

  const value = useMemo<NetworkInfoContextValue>(() => {
    return {
      nodeInfo,
      forkNetwork: isFork ? forkNetwork : undefined,
      isMainnetFork,
      addressFormat,
      network,
    }
  }, [addressFormat, forkNetwork, isFork, isMainnetFork, network, nodeInfo])

  return <NetworkInfoContext.Provider value={value}>{children}</NetworkInfoContext.Provider>
}
