import {useMemo} from "react"

import type {TonClient} from "../explorer/api/client"
import type {V3TransactionListItem} from "../explorer/api/types"
import {
  collectTransactionListAddresses,
  type MessageNamesByAddress,
  useMessageNamesByAddress,
} from "../explorer/hooks/useMessageNamesByAddress"

export function useDeveloperMessageNames(
  client: TonClient,
  transactions: readonly V3TransactionListItem[],
): {
  readonly addresses: readonly string[]
  readonly messageNamesByAddress: MessageNamesByAddress
} {
  const addresses = useMemo(() => collectTransactionListAddresses(transactions), [transactions])
  const messageNamesByAddress = useMessageNamesByAddress({client, addresses})

  return {addresses, messageNamesByAddress}
}
