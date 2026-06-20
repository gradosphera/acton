import type {SyntheticEvent} from "react"

export const TOKEN_PLACEHOLDER_IMAGE = "/token-placeholder.svg"

export const TOKEN_IMAGE_SOURCE_KEYS = [
  "image",
  "_image_small",
  "_image_medium",
  "_image_big",
] as const

export const NFT_IMAGE_SOURCE_KEYS = [
  "image",
  "_image_small",
  "_image_medium",
  "_image_big",
  "preview",
  "image_url",
] as const

export function getImageSources(
  content: Record<string, unknown> | undefined,
  keys: readonly string[] = TOKEN_IMAGE_SOURCE_KEYS,
): string[] {
  const sources: string[] = []
  for (const key of keys) {
    const value = content?.[key]
    if (typeof value === "string" && value.length > 0 && !sources.includes(value)) {
      sources.push(value)
    }
  }
  return sources
}

export function getPrimaryImageSource(
  content: Record<string, unknown> | undefined,
  keys?: readonly string[],
): string {
  return getImageSources(content, keys)[0] ?? TOKEN_PLACEHOLDER_IMAGE
}

export function replaceBrokenImageWithFallback(
  event: SyntheticEvent<HTMLImageElement>,
  sources: readonly string[],
) {
  const image = event.currentTarget
  const currentSource = image.getAttribute("src")
  if (currentSource === TOKEN_PLACEHOLDER_IMAGE) {
    return
  }
  if (isToncenterImageProxyUrl(currentSource)) {
    image.src = TOKEN_PLACEHOLDER_IMAGE
    return
  }

  const candidates = [
    ...sources.filter(source => source !== TOKEN_PLACEHOLDER_IMAGE),
    TOKEN_PLACEHOLDER_IMAGE,
  ]
  const currentIndex = currentSource ? candidates.indexOf(currentSource) : -1
  const nextSource = candidates
    .slice(currentIndex >= 0 ? currentIndex + 1 : 0)
    .find(source => source !== currentSource)

  if (nextSource) {
    image.src = nextSource
  }
}

function isToncenterImageProxyUrl(source: string | null): boolean {
  if (!source) {
    return false
  }

  try {
    return new URL(source, globalThis.location.href).hostname === "imgproxy.toncenter.com"
  } catch {
    return false
  }
}
