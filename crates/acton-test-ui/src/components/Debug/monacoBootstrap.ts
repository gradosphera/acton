import "@codingame/monaco-vscode-theme-defaults-default-extension"

import {initialize} from "@codingame/monaco-vscode-api"
import getConfigurationServiceOverride from "@codingame/monaco-vscode-configuration-service-override"
import getFilesServiceOverride from "@codingame/monaco-vscode-files-service-override"
import getModelServiceOverride from "@codingame/monaco-vscode-model-service-override"
import getThemeServiceOverride from "@codingame/monaco-vscode-theme-service-override"
import * as monaco from "monaco-editor"
import editorWorker from "monaco-editor/esm/vs/editor/editor.worker?worker"

interface MonacoEnvironmentConfig {
  readonly getWorker?: (_moduleId: string, _label: string) => Worker
}

let monacoReadyPromise: Promise<typeof monaco> | undefined

export const prepareMonaco = async () => {
  if (monacoReadyPromise === undefined) {
    ;(
      globalThis as typeof globalThis & {
        MonacoEnvironment?: MonacoEnvironmentConfig
      }
    ).MonacoEnvironment = {
      getWorker: () => new editorWorker(),
    }

    monacoReadyPromise = (async () => {
      await initialize({
        ...getConfigurationServiceOverride(),
        ...getFilesServiceOverride(),
        ...getModelServiceOverride(),
        ...getThemeServiceOverride(),
      })

      monaco.editor.setTheme(
        document.documentElement.classList.contains("dark-theme") ? "vs-dark" : "vs",
      )

      return monaco
    })()
  }

  return monacoReadyPromise
}
