import {useMemo} from "react"
import type {FC, ReactNode} from "react"

import {NetworkInfoContext, type NetworkInfoContextValue} from "./useNetworkInfo"

interface StaticNetworkInfoProviderProps {
  readonly children: ReactNode
  readonly testOnly?: boolean
}

export const StaticNetworkInfoProvider: FC<StaticNetworkInfoProviderProps> = ({
  children,
  testOnly = false,
}) => {
  const addressFormat = useMemo(() => ({testOnly}), [testOnly])
  const value = useMemo<NetworkInfoContextValue>(
    () => ({
      addressFormat,
      isMainnetFork: !testOnly,
    }),
    [addressFormat, testOnly],
  )

  return <NetworkInfoContext.Provider value={value}>{children}</NetworkInfoContext.Provider>
}
