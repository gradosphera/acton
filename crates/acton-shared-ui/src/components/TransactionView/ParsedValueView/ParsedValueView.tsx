import * as React from "react"

import type {ContractData, ParsedValue} from "@/types/transaction"

import {ContractChip} from "../ContractChip/ContractChip"

import styles from "./ParsedValueView.module.css"

const DECIMAL_SCALAR_PATTERN = /^-?\d+(?:\.\d+)?$/

function ParsedTypeLabel({typeName}: {readonly typeName: string}): React.JSX.Element {
  return <span className={styles.parsedTypeLabel}>{typeName}</span>
}

function ParsedValueRow({
  label,
  value,
  contracts,
  onContractClick,
}: {
  readonly label: string
  readonly value: ParsedValue
  readonly contracts: Map<string, ContractData>
  readonly onContractClick?: (address: string) => void
}): React.JSX.Element {
  return (
    <>
      <div className={styles.parsedEntryKey}>{label}:</div>
      <div className={styles.parsedEntryValue}>
        <ParsedValueView value={value} contracts={contracts} onContractClick={onContractClick} />
      </div>
    </>
  )
}

interface ParsedValueViewProps {
  readonly value: ParsedValue
  readonly contracts: Map<string, ContractData>
  readonly onContractClick?: (address: string) => void
  readonly fallbackTypeName?: string
}

export function ParsedValueView({
  value,
  contracts,
  onContractClick,
  fallbackTypeName,
}: ParsedValueViewProps): React.JSX.Element {
  switch (value.kind) {
    case "null": {
      return <span className={styles.parsedNull}>null</span>
    }
    case "address": {
      return (
        <ContractChip
          address={value.value}
          contracts={contracts}
          onContractClick={onContractClick}
        />
      )
    }
    case "boolean": {
      return (
        <span className={value.value ? styles.booleanTrue : styles.booleanFalse}>
          {value.value ? "true" : "false"}
        </span>
      )
    }
    case "scalar": {
      return (
        <span
          className={
            DECIMAL_SCALAR_PATTERN.test(value.value)
              ? styles.parsedPlainScalar
              : styles.parsedScalar
          }
        >
          {value.value}
        </span>
      )
    }
    case "array": {
      if (value.items.length === 0) {
        return <span className={styles.parsedEmpty}>[]</span>
      }

      return (
        <div className={styles.parsedContainer}>
          <span className={styles.parsedBadge}>array</span>
          <div className={styles.parsedNested}>
            {value.items.map((item, index) => (
              <ParsedValueRow
                key={`array-item-${index}`}
                label={`[${index}]`}
                value={item}
                contracts={contracts}
                onContractClick={onContractClick}
              />
            ))}
          </div>
        </div>
      )
    }
    case "object": {
      const typeName = value.typeName ?? fallbackTypeName

      return (
        <div className={styles.parsedContainer}>
          {typeName && <ParsedTypeLabel typeName={typeName} />}
          {value.entries.length === 0 ? (
            <span className={styles.parsedEmpty}>{"{}"}</span>
          ) : (
            <div className={styles.parsedNested}>
              {value.entries.map(entry => (
                <ParsedValueRow
                  key={entry.key}
                  label={entry.key}
                  value={entry.value}
                  contracts={contracts}
                  onContractClick={onContractClick}
                />
              ))}
            </div>
          )}
        </div>
      )
    }
    case "map": {
      return (
        <div className={styles.parsedContainer}>
          <span className={styles.parsedBadge}>map</span>
          {value.entries.length === 0 ? (
            <span className={styles.parsedEmpty}>{"{}"}</span>
          ) : (
            <div className={styles.parsedNested}>
              {value.entries.map((entry, index) => (
                <div key={`map-entry-${index}`} className={styles.parsedMapEntry}>
                  <ParsedValueRow
                    label="key"
                    value={entry.key}
                    contracts={contracts}
                    onContractClick={onContractClick}
                  />
                  <ParsedValueRow
                    label="value"
                    value={entry.value}
                    contracts={contracts}
                    onContractClick={onContractClick}
                  />
                </div>
              ))}
            </div>
          )}
        </div>
      )
    }
  }
}
