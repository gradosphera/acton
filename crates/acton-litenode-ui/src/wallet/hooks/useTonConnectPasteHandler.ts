import {useEffect} from "react"

function isPotentialTonConnectUrl(value: string): boolean {
  const normalized = value.trim().toLowerCase()

  return (
    normalized.startsWith("tonconnect://") ||
    normalized.startsWith("tc://") ||
    normalized.startsWith("ton://") ||
    normalized.startsWith("https://") ||
    normalized.startsWith("http://")
  )
}

export function useTonConnectPasteHandler(handleTonConnectUrl: (url: string) => Promise<void>): void {
  useEffect(() => {
    const handlePaste = async (event: ClipboardEvent) => {
      try {
        const pastedText = event.clipboardData?.getData("text")?.trim()
        if (!pastedText || !isPotentialTonConnectUrl(pastedText)) {
          return
        }

        await handleTonConnectUrl(pastedText)
      } catch {
        // Surface errors through the caller's handler state.
      }
    }

    document.addEventListener("paste", handlePaste)
    return () => document.removeEventListener("paste", handlePaste)
  }, [handleTonConnectUrl])
}
