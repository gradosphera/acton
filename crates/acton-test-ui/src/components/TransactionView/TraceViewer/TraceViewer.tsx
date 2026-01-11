import React, { useEffect, useState, useMemo, useRef, useCallback } from "react"
import { FiX, FiFileText, FiCpu, FiLayers, FiList, FiChevronLeft, FiChevronRight } from "react-icons/fi"
import type { HighLevelTrace, HighLevelTraceStep, DebugLocation } from "../../../types"
import styles from "./TraceViewer.module.css"
import StackViewer, { StackElement } from "./StackViewer"

interface TraceViewerProps {
  readonly contractName: string
  readonly vmLog: string
  readonly onClose: () => void
}

interface FileContent {
  readonly path: string
  readonly lines: string[]
}

const parseStack = (stackStr: string): StackElement[] => {
  if (!stackStr || !stackStr.startsWith("[") || !stackStr.endsWith("]")) {
    return []
  }
  const content = stackStr.slice(1, -1).trim()
  if (!content) return []
  
  const items: string[] = []
  let current = ""
  let depth = 0
  for (let i = 0; i < content.length; i++) {
    const char = content[i]
    if (char === "{" || char === "(" || char === "[") depth++
    else if (char === "}" || char === ")" || char === "]") depth--
    
    if (char === " " && depth === 0) {
      if (current) items.push(current)
      current = ""
    } else {
      current += char
    }
  }
  if (current) items.push(current)

  return items.map((item): StackElement => {
    if (item.startsWith("C{")) {
      return { $: "Cell", value: item.slice(2, -1) }
    }
    if (item.startsWith("CS{")) {
      const isLong = item.includes("bits:")
      if (isLong) {
        // CS{Cell{HEX} bits:a..b ; refs:c..d}
        const hexMatch = item.match(/Cell\{([0-9A-F]+)\}/)
        const bitsMatch = item.match(/bits:(\d+)\.\.(\d+)/)
        const refsMatch = item.match(/refs:(\d+)\.\.(\d+)/)
        return { 
          $: "Slice", 
          value: hexMatch?.[1] || item,
          hex: hexMatch?.[1],
          bits: bitsMatch ? parseInt(bitsMatch[2]) - parseInt(bitsMatch[1]) : undefined,
          refs: refsMatch ? parseInt(refsMatch[2]) - parseInt(refsMatch[1]) : undefined,
        }
      }
      return { $: "Slice", value: item.slice(3, -1) }
    }
    if (item.startsWith("BC{")) {
      return { $: "Builder", value: item.slice(3, -1) }
    }
    if (item.startsWith("Cont{")) {
      return { $: "Continuation", value: item.slice(5, -1) }
    }
    if (item.match(/^-?\d+$/)) {
      return { $: "Integer", value: item }
    }
    if (item === "NaN") return { $: "NaN", value: "NaN" }
    if (item === "()" || item === "null" || item === "(null)") return { $: "Null", value: "null" }
    
    return { $: "Unknown", value: item }
  })
}

export const TraceViewer: React.FC<TraceViewerProps> = ({
  contractName,
  vmLog,
  onClose,
}) => {
  const [trace, setTrace] = useState<HighLevelTrace | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [selectedStepIndex, setSelectedStepIndex] = useState<number>(-1)
  const [showOnlyMapped, setShowOnlyMapped] = useState(true)
  const [showSteps, setShowSteps] = useState(true)
  const [showStack, setShowStack] = useState(true)
  const [files, setFiles] = useState<Record<string, FileContent>>({})
  
  const editorRef = useRef<HTMLDivElement>(null)
  const stepsListRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    setLoading(true)
    fetch("/api/high-level-trace", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ contract_name: contractName, vm_log: vmLog }),
    })
      .then((res) => {
        if (!res.ok) throw new Error("Failed to fetch high-level trace")
        return res.json()
      })
      .then((data: HighLevelTrace) => {
        setTrace(data)
        setLoading(false)
        const firstMapped = data.steps.findIndex((s) => s.type === "mapped")
        if (firstMapped !== -1) {
          setSelectedStepIndex(firstMapped)
        } else if (data.steps.length > 0) {
          setSelectedStepIndex(0)
        }
      })
      .catch((err) => {
        console.error(err)
        setError(err.message)
        setLoading(false)
      })
  }, [contractName, vmLog])

  const filteredSteps = useMemo(() => {
    if (!trace) return []
    return trace.steps
      .map((step, originalIndex) => ({ step, originalIndex }))
      .filter(({ step }) => !showOnlyMapped || step.type === "mapped")
  }, [trace, showOnlyMapped])

  const currentFilteredIndex = useMemo(() => {
    return filteredSteps.findIndex((item) => item.originalIndex === selectedStepIndex)
  }, [filteredSteps, selectedStepIndex])

  const selectStepByIndex = useCallback((newIndex: number) => {
    if (newIndex >= 0 && newIndex < filteredSteps.length) {
      setSelectedStepIndex(filteredSteps[newIndex].originalIndex)
    }
  }, [filteredSteps])

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "ArrowUp" || e.key === "ArrowLeft") {
        e.preventDefault()
        selectStepByIndex(currentFilteredIndex - 1)
      } else if (e.key === "ArrowDown" || e.key === "ArrowRight") {
        e.preventDefault()
        selectStepByIndex(currentFilteredIndex + 1)
      } else if (e.key === "Escape") {
        onClose()
      }
    }

    window.addEventListener("keydown", handleKeyDown)
    return () => window.removeEventListener("keydown", handleKeyDown)
  }, [currentFilteredIndex, selectStepByIndex, onClose])

  const selectedStep = useMemo(() => {
    if (selectedStepIndex === -1 || !trace) return null
    return trace.steps[selectedStepIndex]
  }, [trace, selectedStepIndex])

  const selectedLoc = useMemo(() => {
    if (selectedStep?.type === "mapped" && selectedStep.locs.length > 0) {
      return selectedStep.locs[0]
    }
    return null
  }, [selectedStep])

  const stack = useMemo(() => {
    if (selectedStep?.inner.type === "execute") {
      return parseStack(selectedStep.inner.stack)
    }
    return []
  }, [selectedStep])

  useEffect(() => {
    if (selectedLoc && !files[selectedLoc.loc.file]) {
      fetch(`/api/file?path=${encodeURIComponent(selectedLoc.loc.file)}`)
        .then((res) => {
          if (!res.ok) throw new Error("Failed to fetch file")
          return res.text()
        })
        .then((content) => {
          setFiles((prev) => ({
            ...prev,
            [selectedLoc.loc.file]: {
              path: selectedLoc.loc.file,
              lines: content.split("\n"),
            },
          }))
        })
        .catch((err) => console.error(err))
    }
  }, [selectedLoc, files])

  useEffect(() => {
    if (selectedLoc && editorRef.current) {
      const lineElement = editorRef.current.querySelector(
        `[data-line="${selectedLoc.loc.line}"]`,
      )
      if (lineElement) {
        lineElement.scrollIntoView({ block: "center", behavior: "smooth" })
      }
    }
  }, [selectedLoc])

  useEffect(() => {
    if (selectedStepIndex !== -1 && stepsListRef.current) {
      const stepElement = stepsListRef.current.querySelector(
        `[data-step-index="${selectedStepIndex}"]`,
      )
      if (stepElement) {
        stepElement.scrollIntoView({ block: "nearest", behavior: "smooth" })
      }
    }
  }, [selectedStepIndex])

  if (loading) {
    return (
      <div className={styles.container}>
        <div className={styles.loading}>Generating high-level trace...</div>
      </div>
    )
  }

  if (error) {
    return (
      <div className={styles.container}>
        <div className={styles.header}>
          <div className={styles.title}>Error</div>
          <button type="button" onClick={onClose} className={styles.closeButton}>
            <FiX size={20} />
          </button>
        </div>
        <div className={styles.error}>{error}</div>
      </div>
    )
  }

  const currentFile = selectedLoc ? files[selectedLoc.loc.file] : null

  const renderLineContent = (line: string, lineIdx: number) => {
    if (!selectedLoc) return line || " "
    
    const { line: startLine, column: startCol, end_line: endLine, end_column: endCol } = selectedLoc.loc
    
    if (lineIdx < startLine || lineIdx > endLine) {
      return line || " "
    }

    let start = 0
    if (lineIdx === startLine) {
      start = Math.max(0, startCol + 1)
    }

    let end = line.length
    if (lineIdx === endLine) {
      end = endCol === -1 ? line.length : Math.min(line.length, endCol + 1)
    }
    
    if (start >= end) return line || " "

    return (
      <>
        {line.slice(0, start)}
        <span className={styles.rangeHighlighted}>{line.slice(start, end)}</span>
        {line.slice(end)}
      </>
    )
  }

  return (
    <div className={styles.container}>
      <div className={styles.header}>
        <div className={styles.title}>
          <FiCpu /> VM Trace Viewer — {contractName}
        </div>
        <div className={styles.headerActions}>
          <label className={styles.showMappedLabel}>
            <input
              type="checkbox"
              checked={showOnlyMapped}
              onChange={(e) => setShowOnlyMapped(e.target.checked)}
            />
            Mapped only
          </label>
          <button type="button" onClick={onClose} className={styles.closeButton} title="Close trace viewer">
            <FiX size={20} />
          </button>
        </div>
      </div>

      <div className={styles.content}>
        <div className={`${styles.stepsList} ${!showSteps ? styles.collapsedPane : ""}`}>
          <div className={styles.stepsListHeader}>
            <div className={styles.stepsListTitle}><FiList /> Steps</div>
            <button 
              type="button" 
              className={styles.paneToggleButton} 
              onClick={() => setShowSteps(false)}
              title="Hide steps"
            >
              <FiChevronLeft />
            </button>
          </div>
          <div className={styles.stepsScroll} ref={stepsListRef}>
            {filteredSteps.map(({ step, originalIndex }) => {
              const isMapped = step.type === "mapped"
              const inner = step.inner
              let instr = ""
              let gas = 0

              if (inner.type === "execute") {
                instr = inner.instr
                gas = inner.gas
              } else if (inner.type === "exception") {
                instr = `Exception ${inner.errno}: ${inner.message}`
              } else if (inner.type === "final_c5") {
                instr = "Final C5"
              }

              let locStr = ""
              if (isMapped && step.locs.length > 0) {
                const loc = step.locs[0].loc
                const fileName = loc.file.split("/").pop()
                locStr = `${fileName}:${loc.line + 1}`
              }

              return (
                <div
                  key={originalIndex}
                  data-step-index={originalIndex}
                  className={`${styles.step} ${selectedStepIndex === originalIndex ? styles.stepActive : ""} ${!isMapped ? styles.unmapped : ""}`}
                  onClick={() => setSelectedStepIndex(originalIndex)}
                >
                  <div className={styles.stepHeader}>
                    <span className={styles.instr}>{instr}</span>
                    {gas > 0 && <span className={styles.gas}>{gas} gas</span>}
                  </div>
                  {locStr && <div className={styles.loc}>{locStr}</div>}
                </div>
              )
            })}
          </div>
        </div>

        {!showSteps && (
          <button 
            type="button" 
            className={`${styles.toggleBtnOverlay} ${styles.toggleStepsBtn}`}
            onClick={() => setShowSteps(true)}
            title="Show steps"
          >
            <FiChevronRight />
          </button>
        )}

        <div className={styles.editorContainer}>
          <div className={styles.fileHeader}>
            <FiFileText /> {selectedLoc?.loc.file || "No source file"}
          </div>
          <div className={styles.editor} ref={editorRef}>
            {currentFile ? (
              currentFile.lines.map((line, idx) => (
                <div
                  key={idx}
                  data-line={idx}
                  className={`${styles.line} ${selectedLoc?.loc.line === idx ? styles.lineHighlighted : ""}`}
                >
                  <div className={styles.lineNumber}>{idx + 1}</div>
                  <div className={styles.lineContent}>{renderLineContent(line, idx)}</div>
                </div>
              ))
            ) : (
              <div className={styles.noSelection}>
                {selectedStepIndex === -1
                  ? "Select a step to view source code"
                  : selectedStep?.type === "unmapped"
                    ? "This step is not mapped to source code"
                    : "Loading source code..."}
              </div>
            )}
          </div>
        </div>

        {!showStack && (
          <button 
            type="button" 
            className={`${styles.toggleBtnOverlay} ${styles.toggleStackBtn}`}
            onClick={() => setShowStack(true)}
            title="Show stack"
          >
            <FiChevronLeft />
          </button>
        )}

        <div className={`${styles.stackView} ${!showStack ? styles.collapsedPane : ""}`}>
          <div className={styles.stackHeader}>
            <button 
              type="button" 
              className={styles.paneToggleButton} 
              onClick={() => setShowStack(false)}
              title="Hide stack"
            >
              <FiChevronRight />
            </button>
            <div className={styles.stackTitle}><FiLayers /> Stack</div>
          </div>
          <div className={styles.stackContentWrapper}>
            <StackViewer stack={stack} selectedStepIndex={selectedStepIndex} />
          </div>
        </div>
      </div>
    </div>
  )
}
