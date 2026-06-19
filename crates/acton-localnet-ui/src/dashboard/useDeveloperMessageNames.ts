import type {ContractABI} from "@ton/tolk-abi-to-typescript"
import * as React from "react"

import type {TonClient} from "../explorer/api/client"
import {addressKey, buildMessageNamesByOpcodeHex} from "../explorer/api/compilerAbi"
import type {V3TransactionListItem} from "../explorer/api/types"
import type {DeveloperMessageNamesByAddress} from "../explorer/components/DeveloperTransactionList"

export function useDeveloperMessageNames(
  client: TonClient,
  transactions: readonly V3TransactionListItem[],
): {
  readonly addresses: readonly string[]
  readonly messageNamesByAddress: DeveloperMessageNamesByAddress
} {
  const addresses = React.useMemo(() => collectTransactionAddresses(transactions), [transactions])
  const [compilerAbiByAddress, setCompilerAbiByAddress] = React.useState<
    Map<string, ContractABI | undefined>
  >(new Map())

  React.useEffect(() => {
    let isActive = true

    const loadMessageNames = async () => {
      if (addresses.length === 0) {
        setCompilerAbiByAddress(new Map())
        return
      }

      const states = await client.getAccountStates([...addresses], false).catch(error => {
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
      for (const address of addresses) {
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
  }, [client, addresses])

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

  return {addresses, messageNamesByAddress}
}

function collectTransactionAddresses(transactions: readonly V3TransactionListItem[]): string[] {
  const addresses = new Set<string>()

  for (const transaction of transactions) {
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

  return [...addresses]
}
