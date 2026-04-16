import type React from "react"
import {useCallback, useEffect, useRef, useState} from "react"

import type * as MonacoEditor from "monaco-editor"

import {type TestReport} from "@acton/shared-ui"

import {prepareMonaco} from "./monacoBootstrap"

import styles from "./DebugPage.module.css"

interface ModelReference {
  readonly object: {
    readonly textEditorModel: MonacoEditor.editor.ITextModel
    save: () => Promise<boolean>
  }
  dispose: () => void
}

interface DebugPageProps {
  readonly test: TestReport
  readonly projectRoot?: string
  readonly theme: string
}

interface DapSource {
  readonly name?: string
  readonly path?: string
}

interface DapStackFrame {
  readonly id: number
  readonly name: string
  readonly line: number
  readonly column: number
  readonly source?: DapSource
}

interface DapVariable {
  readonly name: string
  readonly value: string
  readonly type?: string
  readonly variablesReference: number
}

interface ScopeGroup {
  readonly name: string
  readonly variables: readonly DapVariable[]
}

interface DapResponse {
  readonly type: "response"
  readonly request_seq: number
  readonly success?: boolean
  readonly message?: string
  readonly body?: Record<string, unknown>
}

interface DapEvent {
  readonly type: "event"
  readonly event: string
  readonly body?: Record<string, unknown>
}

type DapMessage = DapResponse | DapEvent | Record<string, unknown>

interface PendingRequest {
  readonly resolve: (response: DapResponse) => void
  readonly reject: (error: Error) => void
}

interface PendingEvent {
  readonly predicate: (event: DapEvent) => boolean
  readonly resolve: (event: DapEvent) => void
  readonly reject: (error: Error) => void
  readonly timer: ReturnType<typeof globalThis.setTimeout>
}

const getRelativePath = (filePath: string, projectRoot?: string) => {
  if (projectRoot && filePath.startsWith(projectRoot)) {
    const relativePath = filePath.slice(projectRoot.length)
    return relativePath || filePath
  }

  const parts = filePath.split("/")
  if (parts.length > 4) {
    return `.../${parts.slice(-4).join("/")}`
  }

  return filePath
}

const getWebSocketUrl = (test: TestReport) => {
  const url = new URL("/api/debug/ws", globalThis.location.origin)
  url.protocol = url.protocol === "https:" ? "wss:" : "ws:"
  url.searchParams.set("file_path", test.file_path)
  url.searchParams.set("name", test.name)
  return url.toString()
}

const toError = (error: unknown) => {
  if (error instanceof Error) {
    return error
  }

  return new Error(String(error))
}

export const DebugPage: React.FC<DebugPageProps> = ({test, projectRoot, theme}) => {
  const editorContainerRef = useRef<HTMLDivElement | null>(null)
  const editorRef = useRef<MonacoEditor.editor.IStandaloneCodeEditor | undefined>(undefined)
  const monacoRef = useRef<typeof MonacoEditor | undefined>(undefined)
  const modelReferencesRef = useRef<Map<string, ModelReference>>(new Map())
  const modelChangeDisposablesRef = useRef<Map<string, MonacoEditor.IDisposable>>(new Map())
  const decorationsRef = useRef<string[]>([])
  const websocketRef = useRef<WebSocket | undefined>(undefined)
  const requestSequenceRef = useRef(1)
  const pendingRequestsRef = useRef<Map<number, PendingRequest>>(new Map())
  const pendingEventsRef = useRef<PendingEvent[]>([])
  const queuedEventsRef = useRef<DapEvent[]>([])
  const lastSavedContentsRef = useRef<Map<string, string>>(new Map())
  const activeFilePathRef = useRef(test.file_path)
  const currentLineRef = useRef<number | undefined>(undefined)
  const currentColumnRef = useRef<number | undefined>(undefined)
  const variableContextVersionRef = useRef(0)

  const [isEditorReady, setIsEditorReady] = useState(false)
  const [isDirty, setIsDirty] = useState(false)
  const [isSaving, setIsSaving] = useState(false)
  const [actionPending, setActionPending] = useState(false)
  const [sessionStatus, setSessionStatus] = useState<
    "idle" | "launching" | "connecting" | "ready" | "paused" | "running" | "terminated" | "error"
  >("idle")
  const [sessionError, setSessionError] = useState<string | undefined>()
  const [sessionLogs, setSessionLogs] = useState<string[]>([])
  const [stackFrames, setStackFrames] = useState<readonly DapStackFrame[]>([])
  const [activeFrameId, setActiveFrameId] = useState<number | undefined>()
  const [scopeGroups, setScopeGroups] = useState<readonly ScopeGroup[]>([])
  const [expandedVariables, setExpandedVariables] = useState<Record<number, true>>({})
  const [loadingVariables, setLoadingVariables] = useState<Record<number, true>>({})
  const [variableChildren, setVariableChildren] = useState<Record<number, readonly DapVariable[]>>(
    {},
  )
  const [activeThreadId, setActiveThreadId] = useState<number | undefined>()
  const [activeFilePath, setActiveFilePath] = useState(test.file_path)
  const [currentLine, setCurrentLine] = useState<number | undefined>()
  const [currentColumn, setCurrentColumn] = useState<number | undefined>()

  const appendLog = useCallback((line: string) => {
    setSessionLogs(previous => {
      const next = [...previous, line]
      return next.slice(-400)
    })
  }, [])

  const disposeAllModels = useCallback(() => {
    for (const disposable of modelChangeDisposablesRef.current.values()) {
      disposable.dispose()
    }
    modelChangeDisposablesRef.current.clear()

    for (const modelReference of modelReferencesRef.current.values()) {
      modelReference.dispose()
    }
    modelReferencesRef.current.clear()
    lastSavedContentsRef.current.clear()
  }, [])

  const rejectPendingMessages = useCallback((error: Error) => {
    for (const pending of pendingRequestsRef.current.values()) {
      pending.reject(error)
    }
    pendingRequestsRef.current.clear()

    for (const pending of pendingEventsRef.current) {
      globalThis.clearTimeout(pending.timer)
      pending.reject(error)
    }
    pendingEventsRef.current = []
    queuedEventsRef.current = []
  }, [])

  const resetVariableTree = useCallback(() => {
    const nextVersion = variableContextVersionRef.current + 1
    variableContextVersionRef.current = nextVersion
    setExpandedVariables({})
    setLoadingVariables({})
    setVariableChildren({})
    return nextVersion
  }, [])

  const ensureModelReference = useCallback(async (filePath: string) => {
    const existingModelReference = modelReferencesRef.current.get(filePath)
    if (existingModelReference !== undefined) {
      return existingModelReference
    }

    const response = await fetch(`/api/file?path=${encodeURIComponent(filePath)}`)
    if (!response.ok) {
      throw new Error(`Failed to load source file: ${response.status}`)
    }

    const source = await response.text()
    lastSavedContentsRef.current.set(filePath, source)

    const monaco = await prepareMonaco()
    monacoRef.current = monaco

    const modelReference = (await monaco.editor.createModelReference(
      monaco.Uri.file(filePath),
      source,
    )) as ModelReference

    modelReferencesRef.current.set(filePath, modelReference)

    const model = modelReference.object.textEditorModel
    const changeDisposable = model.onDidChangeContent(() => {
      if (activeFilePathRef.current !== filePath) {
        return
      }

      setIsDirty(model.getValue() !== lastSavedContentsRef.current.get(filePath))
    })
    modelChangeDisposablesRef.current.set(filePath, changeDisposable)

    return modelReference
  }, [])

  const applyCurrentLineDecoration = useCallback((lineNumber?: number, columnNumber?: number) => {
    const editor = editorRef.current
    const monaco = monacoRef.current
    if (editor === undefined || monaco === undefined) {
      return
    }

    decorationsRef.current = editor.deltaDecorations(
      decorationsRef.current,
      lineNumber === undefined
        ? []
        : [
            {
              range: new monaco.Range(lineNumber, 1, lineNumber, 1),
              options: {
                isWholeLine: true,
                className: styles.currentLineDecoration,
                glyphMarginClassName: styles.currentLineGlyph,
              },
            },
          ],
    )

    if (lineNumber !== undefined) {
      const position = {
        lineNumber,
        column: Math.max(columnNumber ?? 1, 1),
      }
      editor.setPosition(position)
      editor.revealPositionInCenter(position)
    }
  }, [])

  const updateCurrentLocation = useCallback(
    (frame?: DapStackFrame) => {
      const framePath = frame?.source?.path
      if (framePath !== undefined && framePath.length > 0) {
        setActiveFilePath(framePath)
      }

      setCurrentLine(frame?.line)
      setCurrentColumn(frame?.column)
      applyCurrentLineDecoration(frame?.line, frame?.column)
    },
    [applyCurrentLineDecoration],
  )

  const stopSession = useCallback(
    (nextStatus: "idle" | "terminated" = "idle") => {
      const socket = websocketRef.current
      websocketRef.current = undefined

      if (socket !== undefined) {
        try {
          if (socket.readyState === WebSocket.OPEN) {
            socket.send(JSON.stringify({type: "stop"}))
          }
          socket.close()
        } catch {
          // ignore best-effort cleanup
        }
      }

      rejectPendingMessages(new Error("Debug session stopped"))
      setSessionStatus(nextStatus)
      setActionPending(false)
      setActiveThreadId(undefined)
      setActiveFrameId(undefined)
      setStackFrames([])
      setScopeGroups([])
      resetVariableTree()
      setCurrentLine(undefined)
      setCurrentColumn(undefined)
      applyCurrentLineDecoration()
    },
    [applyCurrentLineDecoration, rejectPendingMessages, resetVariableTree],
  )

  const sendDapRequest = useCallback((command: string, args?: Record<string, unknown>) => {
    return new Promise<DapResponse>((resolve, reject) => {
      const socket = websocketRef.current
      if (socket === undefined || socket.readyState !== WebSocket.OPEN) {
        reject(new Error("Debug websocket is not connected"))
        return
      }

      const seq = requestSequenceRef.current
      requestSequenceRef.current += 1
      pendingRequestsRef.current.set(seq, {resolve, reject})

      try {
        socket.send(
          JSON.stringify({
            type: "dap",
            payload: {
              seq,
              type: "request",
              command,
              arguments: args,
            },
          }),
        )
      } catch (error) {
        pendingRequestsRef.current.delete(seq)
        reject(toError(error))
      }
    }).then(response => {
      if (response.success === false) {
        throw new Error(response.message ?? `DAP request '${command}' failed`)
      }

      return response
    })
  }, [])

  const loadScopesForFrame = useCallback(
    async (frameId: number) => {
      const contextVersion = resetVariableTree()
      setScopeGroups([])

      const scopesResponse = await sendDapRequest("scopes", {frameId})
      const scopes =
        (scopesResponse.body?.scopes as {name: string; variablesReference: number}[] | undefined) ??
        []

      const groups = await Promise.all(
        scopes.map(async scope => {
          if (scope.variablesReference <= 0) {
            return {name: scope.name, variables: []}
          }

          const variablesResponse = await sendDapRequest("variables", {
            variablesReference: scope.variablesReference,
          })
          return {
            name: scope.name,
            variables: (variablesResponse.body?.variables as DapVariable[] | undefined) ?? [],
          }
        }),
      )

      if (variableContextVersionRef.current !== contextVersion) {
        return
      }

      setScopeGroups(groups)
    },
    [resetVariableTree, sendDapRequest],
  )

  const selectStackFrame = useCallback(
    async (frame: DapStackFrame) => {
      try {
        setSessionError(undefined)
        setActiveFrameId(frame.id)
        updateCurrentLocation(frame)
        await loadScopesForFrame(frame.id)
      } catch (error) {
        const err = toError(error)
        setSessionError(err.message)
        appendLog(`[error] ${err.message}`)
      }
    },
    [appendLog, loadScopesForFrame, updateCurrentLocation],
  )

  const loadVariableChildren = useCallback(
    async (variablesReference: number) => {
      if (variablesReference <= 0 || loadingVariables[variablesReference] === true) {
        return
      }

      if (variableChildren[variablesReference] !== undefined) {
        return
      }

      const contextVersion = variableContextVersionRef.current
      setLoadingVariables(previous => ({...previous, [variablesReference]: true}))

      try {
        const variablesResponse = await sendDapRequest("variables", {variablesReference})
        if (variableContextVersionRef.current !== contextVersion) {
          return
        }

        setVariableChildren(previous => ({
          ...previous,
          [variablesReference]:
            (variablesResponse.body?.variables as DapVariable[] | undefined) ?? [],
        }))
      } finally {
        if (variableContextVersionRef.current === contextVersion) {
          setLoadingVariables(previous => {
            const next = {...previous}
            delete next[variablesReference]
            return next
          })
        }
      }
    },
    [loadingVariables, sendDapRequest, variableChildren],
  )

  const toggleVariableExpansion = useCallback(
    async (variable: DapVariable) => {
      const reference = variable.variablesReference
      if (reference <= 0) {
        return
      }

      if (expandedVariables[reference] === true) {
        setExpandedVariables(previous => {
          const next = {...previous}
          delete next[reference]
          return next
        })
        return
      }

      try {
        await loadVariableChildren(reference)
        setExpandedVariables(previous => ({...previous, [reference]: true}))
      } catch (error) {
        const err = toError(error)
        setSessionError(err.message)
        appendLog(`[error] ${err.message}`)
      }
    },
    [appendLog, expandedVariables, loadVariableChildren],
  )

  const waitForEvent = useCallback(
    (predicate: (event: DapEvent) => boolean, timeoutMs = 15_000) => {
      return new Promise<DapEvent>((resolve, reject) => {
        const queuedEventIndex = queuedEventsRef.current.findIndex(event => predicate(event))
        if (queuedEventIndex !== -1) {
          const [queuedEvent] = queuedEventsRef.current.splice(queuedEventIndex, 1)
          if (queuedEvent !== undefined) {
            resolve(queuedEvent)
            return
          }
        }

        const pendingEvent: PendingEvent = {
          predicate,
          resolve: event => {
            globalThis.clearTimeout(pendingEvent.timer)
            resolve(event)
          },
          reject: error => {
            globalThis.clearTimeout(pendingEvent.timer)
            reject(error)
          },
          timer: globalThis.setTimeout(() => {
            pendingEventsRef.current = pendingEventsRef.current.filter(
              item => item !== pendingEvent,
            )
            reject(new Error("Timed out waiting for debugger event"))
          }, timeoutMs),
        }

        pendingEventsRef.current = [...pendingEventsRef.current, pendingEvent]
      })
    },
    [],
  )

  const refreshPausedState = useCallback(async () => {
    const threadsResponse = await sendDapRequest("threads")
    const threads = (threadsResponse.body?.threads as {id: number}[] | undefined) ?? []
    const threadId = threads[0]?.id ?? 1
    setActiveThreadId(threadId)

    const stackTraceResponse = await sendDapRequest("stackTrace", {threadId})
    const frames = (stackTraceResponse.body?.stackFrames as DapStackFrame[] | undefined) ?? []
    setStackFrames(frames)

    const currentFrame = frames[0]
    setActiveFrameId(currentFrame?.id)
    updateCurrentLocation(currentFrame)

    if (currentFrame === undefined) {
      setScopeGroups([])
      resetVariableTree()
      return
    }

    await loadScopesForFrame(currentFrame.id)
  }, [loadScopesForFrame, resetVariableTree, sendDapRequest, updateCurrentLocation])

  const saveFile = useCallback(async () => {
    const model = modelReferencesRef.current.get(activeFilePath)?.object.textEditorModel
    if (model === undefined) {
      return
    }

    setIsSaving(true)
    try {
      const content = model.getValue()
      const response = await fetch("/api/file", {
        method: "PUT",
        headers: {
          "content-type": "application/json",
        },
        body: JSON.stringify({
          path: activeFilePath,
          content,
        }),
      })

      if (!response.ok) {
        throw new Error(`Failed to save file: ${response.status}`)
      }

      await modelReferencesRef.current.get(activeFilePath)?.object.save()
      lastSavedContentsRef.current.set(activeFilePath, content)
      setIsDirty(false)
      appendLog(`Saved ${getRelativePath(activeFilePath, projectRoot)}`)
    } finally {
      setIsSaving(false)
    }
  }, [activeFilePath, appendLog, projectRoot])

  const handleSave = useCallback(async () => {
    try {
      setSessionError(undefined)
      await saveFile()
    } catch (error) {
      const err = toError(error)
      setSessionError(err.message)
      appendLog(`[error] ${err.message}`)
    }
  }, [appendLog, saveFile])

  const connectDebugSocket = useCallback(async () => {
    const socket = new WebSocket(getWebSocketUrl(test))

    const handleSocketMessage = (event: MessageEvent<string>) => {
      let parsed: Record<string, unknown>
      try {
        parsed = JSON.parse(String(event.data)) as Record<string, unknown>
      } catch {
        appendLog(`Malformed websocket message: ${String(event.data)}`)
        return
      }

      if (parsed.type === "status") {
        const status = parsed.status
        if (typeof status === "string") {
          if (status === "launching" || status === "connecting" || status === "ready") {
            setSessionStatus(status)
          }
          appendLog(`debug:${status}`)
        }
        return
      }

      if (parsed.type === "process-output") {
        const stream = parsed.stream === "stderr" ? "stderr" : "stdout"
        const text = typeof parsed.text === "string" ? parsed.text : ""
        appendLog(`[${stream}] ${text}`)
        return
      }

      if (parsed.type === "process-exit") {
        const exitStatus = typeof parsed.status === "number" ? parsed.status.toString() : "unknown"
        appendLog(`Process exited with status ${exitStatus}`)
        setSessionStatus("terminated")
        return
      }

      if (parsed.type === "error") {
        const message = typeof parsed.message === "string" ? parsed.message : "Unknown error"
        setSessionError(message)
        setSessionStatus("error")
        appendLog(`[error] ${message}`)
        return
      }

      if (parsed.type !== "dap") {
        return
      }

      const payload = parsed.payload as DapMessage | undefined
      if (payload === undefined || typeof payload !== "object") {
        return
      }

      if (payload.type === "response") {
        const response = payload as DapResponse
        const pendingRequest = pendingRequestsRef.current.get(response.request_seq)
        if (pendingRequest !== undefined) {
          pendingRequestsRef.current.delete(response.request_seq)
          pendingRequest.resolve(response)
        }
        return
      }

      if (payload.type === "event") {
        const dapEvent = payload as DapEvent
        switch (dapEvent.event) {
          case "terminated": {
            setSessionStatus("terminated")
            break
          }
          case "stopped": {
            setSessionStatus("paused")
            break
          }
          case "output": {
            const output = dapEvent.body?.output
            if (typeof output === "string" && output.length > 0) {
              appendLog(`[dap] ${output.trimEnd()}`)
            }
            break
          }
          default: {
            break
          }
        }

        const nextWaiters: PendingEvent[] = []
        let wasHandled = false
        for (const waiter of pendingEventsRef.current) {
          if (waiter.predicate(dapEvent)) {
            waiter.resolve(dapEvent)
            wasHandled = true
          } else {
            nextWaiters.push(waiter)
          }
        }
        pendingEventsRef.current = nextWaiters
        if (!wasHandled) {
          queuedEventsRef.current = [...queuedEventsRef.current.slice(-31), dapEvent]
        }
      }
    }

    const handleSocketClose = () => {
      if (websocketRef.current === socket) {
        websocketRef.current = undefined
        rejectPendingMessages(new Error("Debug websocket closed"))
        setActionPending(false)
        setActiveThreadId(undefined)
        setSessionStatus(previous =>
          previous === "error" || previous === "idle" || previous === "terminated"
            ? previous
            : "terminated",
        )
      }
    }

    websocketRef.current = socket
    socket.addEventListener("message", handleSocketMessage)
    socket.addEventListener("close", handleSocketClose)

    await new Promise<void>((resolve, reject) => {
      socket.addEventListener("open", () => resolve(), {once: true})
      socket.addEventListener("error", () => reject(new Error("Failed to open debug websocket")), {
        once: true,
      })
    })

    return socket
  }, [appendLog, rejectPendingMessages, test])

  const startDebugSession = useCallback(async () => {
    try {
      setSessionError(undefined)
      setSessionLogs([])

      if (isDirty) {
        await saveFile()
      }

      stopSession("idle")
      setSessionStatus("launching")

      await connectDebugSocket()
      appendLog(`Launching debug for ${test.name}`)

      await sendDapRequest("initialize", {
        clientID: "acton-test-ui",
        clientName: "Acton Test UI",
        adapterID: "tolk-debugger",
        linesStartAt1: true,
        columnsStartAt1: true,
        supportsVariableType: true,
        supportsVariablePaging: false,
        supportsRunInTerminalRequest: false,
        supportsMemoryReferences: false,
        supportsProgressReporting: false,
        supportsInvalidatedEvent: false,
        supportsMemoryEvent: false,
        supportsArgsCanBeInterpretedByShell: false,
        supportsStartDebuggingRequest: false,
      })
      await waitForEvent(event => event.event === "initialized")
      await sendDapRequest("configurationDone")
      await sendDapRequest("launch", {noDebug: false})

      const launchEvent = await waitForEvent(event => {
        return event.event === "stopped" || event.event === "terminated"
      }, 20_000)

      if (launchEvent.event === "terminated") {
        setSessionStatus("terminated")
        return
      }

      setSessionStatus("paused")
      await refreshPausedState()
    } catch (error) {
      const err = toError(error)
      setSessionError(err.message)
      setSessionStatus("error")
      appendLog(`[error] ${err.message}`)
    }
  }, [
    appendLog,
    connectDebugSocket,
    isDirty,
    refreshPausedState,
    saveFile,
    sendDapRequest,
    stopSession,
    test.name,
    waitForEvent,
  ])

  const executeThreadAction = useCallback(
    async (command: "continue" | "next" | "stepIn" | "stepOut") => {
      if (activeThreadId === undefined) {
        return
      }

      try {
        setActionPending(true)
        setSessionError(undefined)
        setSessionStatus("running")
        await sendDapRequest(command, {threadId: activeThreadId})
        const nextEvent = await waitForEvent(event => {
          return event.event === "stopped" || event.event === "terminated"
        }, 20_000)

        if (nextEvent.event === "terminated") {
          setSessionStatus("terminated")
          setActiveFrameId(undefined)
          setStackFrames([])
          setScopeGroups([])
          resetVariableTree()
          setCurrentLine(undefined)
          setCurrentColumn(undefined)
          applyCurrentLineDecoration()
          return
        }

        setSessionStatus("paused")
        await refreshPausedState()
      } catch (error) {
        const err = toError(error)
        setSessionError(err.message)
        setSessionStatus("error")
        appendLog(`[error] ${err.message}`)
      } finally {
        setActionPending(false)
      }
    },
    [
      activeThreadId,
      appendLog,
      applyCurrentLineDecoration,
      refreshPausedState,
      resetVariableTree,
      sendDapRequest,
      waitForEvent,
    ],
  )

  useEffect(() => {
    void prepareMonaco().then(monaco => {
      monacoRef.current = monaco
      monaco.editor.setTheme(theme === "dark" ? "vs-dark" : "vs")
    })
  }, [theme])

  useEffect(() => {
    activeFilePathRef.current = activeFilePath
  }, [activeFilePath])

  useEffect(() => {
    currentLineRef.current = currentLine
  }, [currentLine])

  useEffect(() => {
    currentColumnRef.current = currentColumn
  }, [currentColumn])

  useEffect(() => {
    stopSession("idle")
    setActiveFrameId(undefined)
    setActiveFilePath(test.file_path)
    activeFilePathRef.current = test.file_path
    setIsDirty(false)
    setCurrentLine(undefined)
    setCurrentColumn(undefined)
    resetVariableTree()
    disposeAllModels()
  }, [disposeAllModels, resetVariableTree, stopSession, test.file_path, test.name])

  useEffect(() => {
    let disposed = false

    const loadModel = async () => {
      setIsEditorReady(false)
      setSessionError(undefined)

      const monaco = await prepareMonaco()
      if (disposed) {
        return
      }

      monacoRef.current = monaco
      const nextModelReference = await ensureModelReference(activeFilePath)
      if (disposed) {
        return
      }

      const model = nextModelReference.object.textEditorModel

      if (editorContainerRef.current === null) {
        throw new Error("Editor container is missing")
      }

      if (editorRef.current === undefined) {
        editorRef.current = monaco.editor.create(editorContainerRef.current, {
          model,
          automaticLayout: true,
          glyphMargin: true,
          fontSize: 14,
          minimap: {enabled: false},
          scrollBeyondLastLine: false,
          roundedSelection: false,
          lineNumbersMinChars: 4,
        })
      } else {
        editorRef.current.setModel(model)
      }

      setIsEditorReady(true)
      setIsDirty(model.getValue() !== lastSavedContentsRef.current.get(activeFilePath))
      applyCurrentLineDecoration(currentLineRef.current, currentColumnRef.current)
    }

    void loadModel().catch(error => {
      const err = toError(error)
      if (!disposed) {
        setSessionError(err.message)
        setSessionStatus("error")
      }
    })

    return () => {
      disposed = true
    }
  }, [activeFilePath, applyCurrentLineDecoration, ensureModelReference])

  useEffect(() => {
    applyCurrentLineDecoration(currentLine, currentColumn)
  }, [applyCurrentLineDecoration, currentColumn, currentLine])

  useEffect(() => {
    return () => {
      stopSession("idle")
      editorRef.current?.dispose()
      editorRef.current = undefined
      disposeAllModels()
    }
  }, [disposeAllModels, stopSession])

  const canControlExecution =
    (sessionStatus === "paused" || sessionStatus === "running") &&
    activeThreadId !== undefined &&
    !actionPending

  const renderVariableRows = useCallback(
    (variables: readonly DapVariable[], keyPrefix: string, depth = 0): React.ReactNode => {
      return variables.map((variable, index) => {
        const reference = variable.variablesReference
        const isExpandable = reference > 0
        const isExpanded = expandedVariables[reference] === true
        const isLoading = loadingVariables[reference] === true
        const children = variableChildren[reference] ?? []

        return (
          <div
            key={`${keyPrefix}:${variable.name}:${reference}:${index}`}
            className={styles.variableNode}
          >
            <div className={styles.variableRow} style={{paddingLeft: `${depth * 14}px`}}>
              {isExpandable ? (
                <button
                  type="button"
                  className={styles.variableToggle}
                  onClick={() => void toggleVariableExpansion(variable)}
                  aria-label={`${isExpanded ? "Collapse" : "Expand"} ${variable.name}`}
                  aria-expanded={isExpanded}
                >
                  {isExpanded ? "▾" : "▸"}
                </button>
              ) : (
                <span className={styles.variableToggleSpacer} />
              )}

              <div className={styles.variableSummary}>
                <span className={styles.variableName}>{variable.name}</span>
                {variable.type && <span className={styles.variableType}>{variable.type}</span>}
              </div>

              <span className={styles.variableValue}>{variable.value}</span>
            </div>

            {isExpandable && isExpanded && (
              <div className={styles.variableChildren}>
                {isLoading && children.length === 0 ? (
                  <div
                    className={styles.variableLoading}
                    style={{paddingLeft: `${(depth + 1) * 14}px`}}
                  >
                    Loading...
                  </div>
                ) : children.length === 0 ? (
                  <div className={styles.scopeEmpty} style={{paddingLeft: `${(depth + 1) * 14}px`}}>
                    Empty
                  </div>
                ) : (
                  renderVariableRows(children, `${keyPrefix}:${reference}`, depth + 1)
                )}
              </div>
            )}
          </div>
        )
      })
    },
    [expandedVariables, loadingVariables, toggleVariableExpansion, variableChildren],
  )

  return (
    <div className={styles.debugPage}>
      <div className={styles.toolbar}>
        <div className={styles.toolbarPrimary}>
          <div className={styles.titleBlock}>
            <div className={styles.title}>Debug</div>
            <div className={styles.subtitle}>
              <span className={styles.testName}>{test.name}</span>
              <span className={styles.filePath}>
                {getRelativePath(activeFilePath, projectRoot)}
              </span>
            </div>
          </div>
          <span className={`${styles.statusChip} ${styles[`status_${sessionStatus}`]}`}>
            {sessionStatus}
          </span>
          {isDirty && <span className={styles.dirtyBadge}>unsaved changes</span>}
        </div>

        <div className={styles.toolbarActions}>
          <button
            type="button"
            className={styles.actionButton}
            onClick={() => void handleSave()}
            disabled={!isDirty || isSaving}
          >
            {isSaving ? "Saving..." : "Save"}
          </button>
          <button
            type="button"
            className={`${styles.actionButton} ${styles.primaryButton}`}
            onClick={() => void startDebugSession()}
            disabled={!isEditorReady || isSaving || actionPending}
          >
            {sessionStatus === "idle" || sessionStatus === "terminated"
              ? "Start Debug"
              : "Restart Debug"}
          </button>
          <button
            type="button"
            className={styles.actionButton}
            onClick={() => void executeThreadAction("continue")}
            disabled={!canControlExecution || sessionStatus !== "paused"}
          >
            Continue
          </button>
          <button
            type="button"
            className={styles.actionButton}
            onClick={() => void executeThreadAction("next")}
            disabled={!canControlExecution || sessionStatus !== "paused"}
          >
            Step Over
          </button>
          <button
            type="button"
            className={styles.actionButton}
            onClick={() => void executeThreadAction("stepIn")}
            disabled={!canControlExecution || sessionStatus !== "paused"}
          >
            Step In
          </button>
          <button
            type="button"
            className={styles.actionButton}
            onClick={() => void executeThreadAction("stepOut")}
            disabled={!canControlExecution || sessionStatus !== "paused"}
          >
            Step Out
          </button>
          <button
            type="button"
            className={styles.actionButton}
            onClick={() => stopSession("terminated")}
            disabled={websocketRef.current === undefined}
          >
            Stop
          </button>
        </div>
      </div>

      {sessionError && <div className={styles.errorBanner}>{sessionError}</div>}

      <div className={styles.layout}>
        <div className={styles.editorPanel}>
          <div ref={editorContainerRef} className={styles.editorSurface} />
        </div>

        <div className={styles.sidebar}>
          <section className={styles.sidebarSection}>
            <div className={styles.sectionTitle}>Stack</div>
            {stackFrames.length === 0 ? (
              <div className={styles.emptyState}>No stack frames yet.</div>
            ) : (
              <div className={styles.frameList}>
                {stackFrames.map(frame => (
                  <button
                    key={frame.id}
                    type="button"
                    className={`${styles.frameItem} ${frame.id === activeFrameId ? styles.frameItemActive : ""}`}
                    onClick={() => void selectStackFrame(frame)}
                  >
                    <span className={styles.frameName}>{frame.name}</span>
                    <span className={styles.frameMeta}>
                      {frame.source?.name ?? frame.source?.path ?? "unknown"}:{frame.line}:
                      {frame.column}
                    </span>
                  </button>
                ))}
              </div>
            )}
          </section>

          <section className={styles.sidebarSection}>
            <div className={styles.sectionTitle}>Variables</div>
            {scopeGroups.length === 0 ? (
              <div className={styles.emptyState}>No variables loaded.</div>
            ) : (
              <div className={styles.scopeList}>
                {scopeGroups.map(scope => (
                  <div key={scope.name} className={styles.scopeGroup}>
                    <div className={styles.scopeName}>{scope.name}</div>
                    {scope.variables.length === 0 ? (
                      <div className={styles.scopeEmpty}>Empty</div>
                    ) : (
                      <div className={styles.variableList}>
                        {renderVariableRows(scope.variables, scope.name)}
                      </div>
                    )}
                  </div>
                ))}
              </div>
            )}
          </section>
        </div>
      </div>

      <section className={styles.logsSection}>
        <div className={styles.sectionTitle}>Session Log</div>
        {sessionLogs.length === 0 ? (
          <div className={styles.emptyState}>Debugger output will appear here.</div>
        ) : (
          <div className={styles.logList}>
            {sessionLogs.map((line, index) => (
              <div key={`${index}-${line}`} className={styles.logLine}>
                {line}
              </div>
            ))}
          </div>
        )}
      </section>
    </div>
  )
}
