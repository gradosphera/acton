import type {FC} from "react"

import {useAddressName} from "../hooks/useAddressBook"
import {useAddressFormat} from "../hooks/useNetworkInfo"

import {formatAddress} from "./utils"

interface AddressLabelProps {
  readonly address: string
  readonly shorten?: boolean
  readonly fallback?: string
  readonly className?: string
}

export const AddressLabel: FC<AddressLabelProps> = ({
  address,
  shorten = true,
  fallback = "Unknown",
  className,
}) => {
  const addressFormat = useAddressFormat()
  const name = useAddressName(address)

  if (!address) {
    return <span className={className}>{fallback}</span>
  }

  const label = name || formatAddress(address, shorten, addressFormat)
  return <span className={className}>{label}</span>
}
