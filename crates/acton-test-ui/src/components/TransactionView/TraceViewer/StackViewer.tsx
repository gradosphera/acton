import React, { type JSX, memo, useState, useRef, useEffect } from "react"
import { FiCopy } from "react-icons/fi"
import styles from "./StackViewer.module.css"

const truncateMiddle = (text: string, maxLength = 30): JSX.Element => {
  if (text.length <= maxLength) return <>{text}</>

  const partLength = Math.floor(maxLength / 2)
  const start = text.substring(0, partLength)
  const end = text.substring(text.length - partLength)

  return (
    <span title={text} className={styles.truncatedMiddle}>
      {start}
      <span className={styles.ellipsis}>…</span>
      {end}
    </span>
  )
}

export type StackElementType = "Null" | "NaN" | "Integer" | "Cell" | "Slice" | "Builder" | "Continuation" | "Address" | "Tuple" | "Unknown"

export interface StackElement {
  readonly $: StackElementType
  readonly value: string
  readonly boc?: string
  readonly hex?: string
  readonly elements?: StackElement[]
  readonly bits?: number
  readonly refs?: number
  readonly name?: string
}

interface StackViewerProps {
  readonly stack: readonly StackElement[]
  readonly title?: string
  readonly selectedStepIndex?: number
}

const CopyButton: React.FC<{ value: string; title: string; className?: string }> = ({ value, title, className }) => {
  const [copied, setCopied] = useState(false)
  
  const handleCopy = (e: React.MouseEvent) => {
    e.stopPropagation()
    navigator.clipboard.writeText(value)
    setCopied(true)
    setTimeout(() => setCopied(false), 2000)
  }

  return (
    <button 
      type="button" 
      onClick={handleCopy} 
      className={`${styles.copyButton} ${className}`} 
      title={copied ? "Copied!" : title}
    >
      <FiCopy size={12} />
    </button>
  )
}

const StackViewer: React.FC<StackViewerProps> = ({ stack, title, selectedStepIndex }) => {
  const [expandedItem, setExpandedItem] = useState<string | null>(null)
  const containerRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (containerRef.current) {
      containerRef.current.scrollTop = 0
    }
  }, [selectedStepIndex])

  const toggleExpand = (key: string) => {
    setExpandedItem((prev) => (prev === key ? null : key))
  }

  const renderStackElement = (
    element: StackElement,
    keyPrefix: string,
    originalIndex: number,
  ): JSX.Element => {
    const handleItemClick = () => {
      toggleExpand(keyPrefix)
    }

    const handleKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
      if (event.key === "Enter" || event.key === " ") {
        handleItemClick()
      }
    }

    const isExpanded = expandedItem === keyPrefix

    switch (element.$) {
      case "Null":
        return <div className={styles.nullItem} key={keyPrefix}>null</div>
      case "NaN":
        return <div className={styles.nanItem} key={keyPrefix}>NaN</div>
      case "Integer": {
        const val = element.value
        let hexPresentation = ""
        try {
          const bigIntVal = BigInt(val)
          hexPresentation = bigIntVal < 0n 
            ? `-0x${(-bigIntVal).toString(16)}` 
            : `0x${bigIntVal.toString(16)}`
        } catch {
          // ignore
        }
        
        return (
          <div className={styles.integerItem} key={keyPrefix}>
            {val} {hexPresentation && <span className={styles.integerItemHexValue}>({hexPresentation})</span>}
            <CopyButton className={styles.integerItemCopyButton} title="Copy integer value" value={val} />
          </div>
        )
      }
      case "Cell": {
        const boc = element.boc || element.value
        return (
          <div
            className={styles.cellItem}
            key={keyPrefix}
            onClick={handleItemClick}
            onKeyDown={handleKeyDown}
            role="button"
            tabIndex={0}
          >
            <div className={styles.stackItemLabel}>Cell</div>
            <div className={styles.stackItemValue}>
              {isExpanded ? boc : truncateMiddle(boc, 35)}
              {(element.bits !== undefined || element.refs !== undefined) && (
                <div className={styles.stackItemDetails}>
                  Bits: {element.bits ?? 0}, Refs: {element.refs ?? 0}
                </div>
              )}
              <CopyButton className={styles.cellItemCopyButton} title="Copy cell as BoC" value={boc} />
            </div>
          </div>
        )
      }
      case "Slice": {
        const hex = element.hex || element.value
        return (
          <div
            className={styles.sliceItem}
            key={keyPrefix}
            onClick={handleItemClick}
            onKeyDown={handleKeyDown}
            role="button"
            tabIndex={0}
          >
            <div className={styles.stackItemLabel}>Slice</div>
            <div className={styles.stackItemValue}>
              {isExpanded ? hex : truncateMiddle(hex, 35)}
              <CopyButton className={styles.sliceItemCopyButton} title="Copy slice as BoC" value={hex} />
            </div>
            {(element.bits !== undefined || element.refs !== undefined) && (
              <div className={styles.stackItemDetails}>
                Bits: {element.bits ?? 0}, Refs: {element.refs ?? 0}
              </div>
            )}
          </div>
        )
      }
      case "Builder": {
        const hex = element.hex || element.value
        return (
          <div
            className={styles.builderItem}
            key={keyPrefix}
            onClick={handleItemClick}
            onKeyDown={handleKeyDown}
            role="button"
            tabIndex={0}
          >
            <div className={styles.stackItemLabel}>Builder</div>
            <div className={styles.stackItemValue}>
              {isExpanded ? hex : truncateMiddle(hex, 35)}
            </div>
            <div className={styles.stackItemDetails}>
              Bits: {element.bits ?? 0}, Refs: {element.refs ?? 0}
            </div>
            <CopyButton className={styles.builderItemCopyButton} title="Copy builder as BoC" value={hex} />
          </div>
        )
      }
      case "Continuation": {
        const name = element.name || element.value
        return (
          <div
            className={styles.continuationItem}
            key={keyPrefix}
            onClick={() => toggleExpand(keyPrefix)}
            onKeyDown={(e) => {
              if (e.key === "Enter" || e.key === " ") toggleExpand(keyPrefix)
            }}
            role="button"
            tabIndex={0}
          >
            <div className={styles.stackItemLabel}>Continuation</div>
            <div className={styles.stackItemValue}>
              {isExpanded ? name : truncateMiddle(name, 35)}
            </div>
            <CopyButton className={styles.continuationItemCopyButton} title="Copy continuation" value={name} />
          </div>
        )
      }
      case "Address": {
        return (
          <div
            className={styles.addressItem}
            key={keyPrefix}
            onClick={handleItemClick}
            onKeyDown={handleKeyDown}
            role="button"
            tabIndex={0}
          >
            <div className={styles.stackItemLabel}>Address</div>
            <div className={styles.stackItemValue}>
              {isExpanded ? element.value : truncateMiddle(element.value, 35)}
            </div>
            <CopyButton className={styles.addressItemCopyButton} title="Copy address" value={element.value} />
          </div>
        )
      }
      case "Tuple":
        return (
          <div className={styles.tupleItem} key={keyPrefix}>
            <div className={styles.stackItemLabel}>Tuple</div>
            <div className={styles.stackItems}>
              {element.elements?.map((el, i) => {
                const nestedKeyPrefix = `${keyPrefix}-${i}`
                return (
                  <div className={styles.tupleElement} key={nestedKeyPrefix}>
                    {renderStackElement(el, nestedKeyPrefix, originalIndex)}
                  </div>
                )
              })}
            </div>
          </div>
        )
      case "Unknown":
      default:
        return (
          <div className={styles.unknownItem} key={keyPrefix} role="button" tabIndex={0} onClick={handleItemClick}>
            <div className={styles.stackItemLabel}>Unknown</div>
            <div className={styles.stackItemValue}>
              {isExpanded ? element.value : truncateMiddle(element.value, 35)}
            </div>
          </div>
        )
    }
  }

  return (
    <div className={styles.stackViewer}>
      {title && <h3 className={styles.stackTitle}>{title}</h3>}
      <div className={styles.stackContainer} ref={containerRef}>
        {stack.length === 0 ? (
          <div className={styles.emptyStack}>Empty stack</div>
        ) : (
          <div className={styles.stackItems}>
            {[...stack].reverse().map((element, index) => {
              const originalIndex = stack.length - 1 - index
              const key = `${element.$}-${originalIndex}`
              return (
                <div key={key} className={styles.stackElement}>
                  <div className={styles.stackIndex}>{originalIndex}</div>
                  {renderStackElement(element, key, originalIndex)}
                </div>
              )
            })}
          </div>
        )}
      </div>
    </div>
  )
}

export default memo(StackViewer)
