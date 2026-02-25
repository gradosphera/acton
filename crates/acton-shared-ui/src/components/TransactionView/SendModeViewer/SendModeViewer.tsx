import * as React from "react"

import {Tooltip} from "@/components/Tooltip/Tooltip"
import {parseSendMode, type SendModeInfo} from "@/components/TransactionView/SendModeViewer/parser"

import styles from "./SendModeViewer.module.css"

export type {SendModeInfo} from "@/components/TransactionView/SendModeViewer/parser"

interface SendModeViewerProps {
  readonly mode: number | undefined
}

function renderFlags(flags: readonly SendModeInfo[]): React.JSX.Element {
  return (
    <>
      {flags.map((flag, index) => (
        <span key={`${flag.name}-${flag.value}`}>
          {index > 0 && <span className={styles.plus}> + </span>}
          <span className={styles.constant}>
            {flag.name} ({flag.value})
          </span>
        </span>
      ))}
    </>
  )
}

export const SendModeViewer: React.FC<SendModeViewerProps> = ({mode}) => {
  if (mode === undefined) {
    return <span className={styles.empty}>No mode</span>
  }

  const flags = parseSendMode(mode)
  const tooltipContent = (
    <div className={styles.tooltipContent}>
      {flags.map(flag => (
        <div key={`${flag.name}-${flag.value}`} className={styles.tooltipDescription}>
          {flag.description}
        </div>
      ))}
    </div>
  )

  return (
    <Tooltip content={tooltipContent} variant="hover">
      <div className={styles.container}>{renderFlags(flags)}</div>
    </Tooltip>
  )
}
