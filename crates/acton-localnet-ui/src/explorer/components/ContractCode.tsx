import {Buffer} from "node:buffer"

import {
  decodeStorageDataCell,
  jetbrainsDarculaTheme,
  jetbrainsLightTheme,
  ParsedValueView,
  type ContractData,
} from "@acton/shared-ui"
import {Cell} from "@ton/core"
import type {ContractABI} from "@ton/tolk-abi-to-typescript"
import {Check, Copy} from "lucide-react"
import type React from "react"
import {useEffect, useMemo, useState} from "react"
import {createHighlighterCore} from "shiki/core"
import {createOnigurumaEngine} from "shiki/engine/oniguruma"
import type {LanguageRegistration} from "shiki/types"
import {Cell as Cell2, runtime, text} from "ton-assembly"

import tasmGrammarRaw from "../../../../../docs/grammars/grammar-tasm.json"

import styles from "./ContractCode.module.css"

interface ContractCodeProps {
  readonly codeBoc: string
  readonly dataBoc?: string
  readonly compilerAbi?: ContractABI
  readonly compilerAbiLoading?: boolean
  readonly compilerAbiError?: string
  readonly onContractClick?: (address: string) => void
}

type ContractCodeTab = "storage" | "source" | "abi"
type StorageTab = "parsed" | "base64" | "hex" | "hex-hash" | "base64-hash"
type SourceTab = "decompiled" | "base64" | "hex" | "hex-hash" | "base64-hash"
type HighlightLanguage = "tasm" | "json"

const tasmGrammar: LanguageRegistration = {
  ...tasmGrammarRaw,
  name: "tasm",
}

let contractCodeHighlighterPromise: ReturnType<typeof createHighlighterCore> | undefined

const getContractCodeHighlighter = () => {
  contractCodeHighlighterPromise ??= createHighlighterCore({
    themes: [jetbrainsLightTheme, jetbrainsDarculaTheme],
    langs: [tasmGrammar, import("shiki/langs/json.mjs")],
    engine: createOnigurumaEngine(() => import("shiki/wasm")),
  })

  return contractCodeHighlighterPromise
}

export const ContractCode: React.FC<ContractCodeProps> = ({
  codeBoc,
  dataBoc,
  compilerAbi,
  compilerAbiLoading = false,
  compilerAbiError,
  onContractClick,
}) => {
  const [activeTab, setActiveTab] = useState<ContractCodeTab>("storage")
  const [activeStorageTab, setActiveStorageTab] = useState<StorageTab>("parsed")
  const [activeSourceTab, setActiveSourceTab] = useState<SourceTab>("decompiled")

  const codeData = useMemo(() => {
    if (!codeBoc) return
    try {
      const buf = Buffer.from(codeBoc, "base64")
      const cell = Cell2.fromBoc(buf)[0]
      const codeCell = Cell.fromBase64(codeBoc)
      const decompiled = text.print(runtime.decompileCell(cell))

      return {
        base64: codeBoc,
        codeHashBase64: codeCell.hash().toString("base64"),
        codeHashHex: codeCell.hash().toString("hex"),
        hex: buf.toString("hex").toUpperCase(),
        decompiled: decompiled,
      }
    } catch (error) {
      console.error("Failed to process contract code:", error)
      return {
        base64: codeBoc,
        codeHashBase64: "Error processing code hash",
        codeHashHex: "Error processing code hash",
        hex: "Error processing HEX",
        decompiled: "Error: Failed to decompile code.",
      }
    }
  }, [codeBoc])

  const parsedStorage = useMemo(
    () => decodeStorageDataCell(dataBoc, compilerAbi),
    [dataBoc, compilerAbi],
  )
  const storageData = useMemo(() => {
    if (!dataBoc) return
    try {
      const buf = Buffer.from(dataBoc, "base64")
      const dataCell = Cell.fromBase64(dataBoc)

      return {
        base64: dataBoc,
        dataHashBase64: dataCell.hash().toString("base64"),
        dataHashHex: dataCell.hash().toString("hex"),
        hex: buf.toString("hex").toUpperCase(),
      }
    } catch (error) {
      console.error("Failed to process contract data:", error)
      return {
        base64: dataBoc,
        dataHashBase64: "Error processing data hash",
        dataHashHex: "Error processing data hash",
        hex: "Error processing data HEX",
      }
    }
  }, [dataBoc])
  const contracts = useMemo(() => new Map<string, ContractData>(), [])
  const abiJson = useMemo(() => {
    if (!compilerAbi) return
    return JSON.stringify(compilerAbi, undefined, 2)
  }, [compilerAbi])
  const storageUnavailableMessage = compilerAbi
    ? dataBoc
      ? "Storage data could not be decoded with this ABI."
      : "No storage data available for this account."
    : "No compiler ABI registered for storage decoding."

  if (!codeBoc || !codeData) {
    return (
      <div className={styles.container}>
        <div className={styles.empty}>No code available for this account.</div>
      </div>
    )
  }

  return (
    <div className={styles.container}>
      <div className={styles.tabs}>
        <button
          type="button"
          className={`${styles.tab} ${activeTab === "storage" ? styles.tabActive : ""}`}
          onClick={() => setActiveTab("storage")}
        >
          Storage
        </button>
        <button
          type="button"
          className={`${styles.tab} ${activeTab === "source" ? styles.tabActive : ""}`}
          onClick={() => setActiveTab("source")}
        >
          Source
        </button>
        <button
          type="button"
          className={`${styles.tab} ${activeTab === "abi" ? styles.tabActive : ""}`}
          onClick={() => setActiveTab("abi")}
        >
          ABI
        </button>
      </div>

      <div className={styles.content}>
        {activeTab === "storage" ? (
          <StoragePanel
            activeTab={activeStorageTab}
            onTabChange={setActiveStorageTab}
            storageData={storageData}
            parsedStorage={parsedStorage}
            contracts={contracts}
            onContractClick={onContractClick}
            unavailableMessage={storageUnavailableMessage}
          />
        ) : activeTab === "abi" ? (
          compilerAbiError ? (
            <div className={styles.empty}>Failed to load compiler ABI: {compilerAbiError}</div>
          ) : compilerAbiLoading ? (
            <div className={styles.empty}>Loading compiler ABI...</div>
          ) : abiJson ? (
            <ContractTextPanel title="ABI" value={abiJson} language="json" />
          ) : (
            <div className={styles.empty}>No compiler ABI registered for this contract.</div>
          )
        ) : (
          <SourcePanel
            activeTab={activeSourceTab}
            onTabChange={setActiveSourceTab}
            codeData={codeData}
          />
        )}
      </div>
    </div>
  )
}

function StoragePanel({
  activeTab,
  onTabChange,
  storageData,
  parsedStorage,
  contracts,
  onContractClick,
  unavailableMessage,
}: {
  readonly activeTab: StorageTab
  readonly onTabChange: (tab: StorageTab) => void
  readonly storageData?: {
    readonly base64: string
    readonly dataHashBase64: string
    readonly dataHashHex: string
    readonly hex: string
  }
  readonly parsedStorage?: ReturnType<typeof decodeStorageDataCell>
  readonly contracts: Map<string, ContractData>
  readonly onContractClick?: (address: string) => void
  readonly unavailableMessage: string
}): React.JSX.Element {
  const storageTabs: readonly {tab: StorageTab; label: string}[] = [
    {tab: "parsed", label: "parsed"},
    {tab: "base64", label: "base64"},
    {tab: "hex", label: "hex"},
    {tab: "hex-hash", label: "hex hash"},
    {tab: "base64-hash", label: "base64 hash"},
  ]
  const activeStorage =
    activeTab === "base64"
      ? {
          title: "Data BoC Base64",
          value: storageData?.base64,
          wrap: true,
        }
      : activeTab === "hex"
        ? {
            title: "Data BoC HEX",
            value: storageData?.hex,
            wrap: true,
          }
        : activeTab === "hex-hash"
          ? {
              title: "Data hash HEX",
              value: storageData?.dataHashHex,
              wrap: true,
            }
          : activeTab === "base64-hash"
            ? {
                title: "Data hash Base64",
                value: storageData?.dataHashBase64,
                wrap: true,
              }
            : undefined

  return (
    <section className={styles.sourceShell}>
      <div className={styles.editorTabBar}>
        {storageTabs.map(item => (
          <button
            key={item.tab}
            type="button"
            className={`${styles.editorTab} ${activeTab === item.tab ? styles.editorTabActive : ""}`}
            onClick={() => onTabChange(item.tab)}
          >
            {item.label}
          </button>
        ))}
      </div>
      {activeTab === "parsed" ? (
        parsedStorage ? (
          <section className={styles.dataPanel}>
            <div className={styles.storageBlock}>
              <ParsedValueView
                value={parsedStorage.value}
                contracts={contracts}
                onContractClick={onContractClick}
                fallbackTypeName={parsedStorage.name}
              />
            </div>
          </section>
        ) : (
          <div className={styles.empty}>{unavailableMessage}</div>
        )
      ) : activeStorage?.value ? (
        <ContractTextPanel
          title={activeStorage.title}
          value={activeStorage.value}
          wrap={activeStorage.wrap}
        />
      ) : (
        <div className={styles.empty}>No storage data available for this account.</div>
      )}
    </section>
  )
}

function SourcePanel({
  activeTab,
  onTabChange,
  codeData,
}: {
  readonly activeTab: SourceTab
  readonly onTabChange: (tab: SourceTab) => void
  readonly codeData: {
    readonly base64: string
    readonly codeHashBase64: string
    readonly codeHashHex: string
    readonly hex: string
    readonly decompiled: string
  }
}): React.JSX.Element {
  const sourceTabs: readonly {tab: SourceTab; label: string}[] = [
    {tab: "decompiled", label: "disasm"},
    {tab: "base64", label: "base64"},
    {tab: "hex", label: "hex"},
    {tab: "hex-hash", label: "hex hash"},
    {tab: "base64-hash", label: "base64 hash"},
  ]
  const activeSource =
    activeTab === "decompiled"
      ? {
          title: "Disassembly",
          value: codeData.decompiled,
          language: "tasm" as const,
          wrap: false,
        }
      : activeTab === "base64"
        ? {
            title: "Code BoC Base64",
            value: codeData.base64,
            wrap: true,
          }
        : activeTab === "hex"
          ? {
              title: "Code BoC HEX",
              value: codeData.hex,
              wrap: true,
            }
          : activeTab === "hex-hash"
            ? {
                title: "Code hash HEX",
                value: codeData.codeHashHex,
                wrap: true,
              }
            : {
                title: "Code hash Base64",
                value: codeData.codeHashBase64,
                wrap: true,
              }

  return (
    <section className={styles.sourceShell}>
      <div className={styles.editorTabBar}>
        {sourceTabs.map(item => (
          <button
            key={item.tab}
            type="button"
            className={`${styles.editorTab} ${activeTab === item.tab ? styles.editorTabActive : ""}`}
            onClick={() => onTabChange(item.tab)}
          >
            {item.label}
          </button>
        ))}
      </div>
      <ContractTextPanel
        title={activeSource.title}
        value={activeSource.value}
        language={activeSource.language}
        wrap={activeSource.wrap}
      />
    </section>
  )
}

function ContractTextPanel({
  title,
  value,
  language,
  wrap = false,
}: {
  readonly title: string
  readonly value: string
  readonly language?: HighlightLanguage
  readonly wrap?: boolean
}): React.JSX.Element {
  return (
    <section className={styles.dataPanel}>
      <CopyTextButton className={styles.copyButton} title={title} value={value} />
      <CodeContent value={value} language={language} wrap={wrap} />
    </section>
  )
}

function CopyTextButton({
  className,
  title,
  value,
}: {
  readonly className: string
  readonly title: string
  readonly value: string
}): React.JSX.Element {
  const [isCopied, setIsCopied] = useState(false)

  useEffect(() => {
    if (!isCopied) {
      return
    }

    const timer = setTimeout(() => setIsCopied(false), 1600)
    return () => clearTimeout(timer)
  }, [isCopied])

  return (
    <button
      type="button"
      className={className}
      onClick={() => {
        void navigator.clipboard.writeText(value)
        setIsCopied(true)
      }}
      aria-label={isCopied ? `${title} copied` : `Copy ${title}`}
      title={isCopied ? "Copied" : `Copy ${title}`}
    >
      {isCopied ? <Check size={14} /> : <Copy size={14} />}
    </button>
  )
}

function CodeContent({
  value,
  language,
  wrap,
}: {
  readonly value: string
  readonly language?: HighlightLanguage
  readonly wrap: boolean
}): React.JSX.Element {
  if (language) {
    return <HighlightedCode value={value} language={language} wrap={wrap} />
  }

  return (
    <pre className={`${styles.code} ${wrap ? styles.codeWrap : ""}`}>
      <code>{value}</code>
    </pre>
  )
}

function HighlightedCode({
  value,
  language,
  wrap,
}: {
  readonly value: string
  readonly language: HighlightLanguage
  readonly wrap: boolean
}): React.JSX.Element {
  const [highlightedHtml, setHighlightedHtml] = useState<string | undefined>()

  useEffect(() => {
    let isActive = true

    const highlight = async () => {
      setHighlightedHtml(undefined)
      try {
        const highlighter = await getContractCodeHighlighter()
        const isDark = document.documentElement.classList.contains("dark-theme")
        const html = highlighter.codeToHtml(value, {
          lang: language,
          theme: isDark ? "jetbrains-darcula" : "jetbrains-light",
        })

        if (isActive) {
          setHighlightedHtml(html)
        }
      } catch (error) {
        console.error("Failed to highlight contract code:", error)
        if (isActive) {
          setHighlightedHtml(undefined)
        }
      }
    }

    void highlight()

    const observer = new MutationObserver(mutations => {
      for (const mutation of mutations) {
        if (mutation.type === "attributes" && mutation.attributeName === "class") {
          void highlight()
        }
      }
    })
    observer.observe(document.documentElement, {attributes: true})

    return () => {
      isActive = false
      observer.disconnect()
    }
  }, [language, value])

  if (!highlightedHtml) {
    return (
      <pre className={`${styles.code} ${wrap ? styles.codeWrap : ""}`}>
        <code>{value}</code>
      </pre>
    )
  }

  return (
    <div
      className={`${styles.highlightedCode} ${wrap ? styles.highlightedCodeWrap : ""}`}
      dangerouslySetInnerHTML={{__html: highlightedHtml}}
    />
  )
}
