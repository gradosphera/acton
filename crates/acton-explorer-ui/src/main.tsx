import {StrictMode} from "react"
import {createRoot} from "react-dom/client"

import {ExplorerApp} from "./ExplorerApp"

const rootElement = document.querySelector("#root")
if (rootElement) {
  createRoot(rootElement).render(
    <StrictMode>
      <ExplorerApp />
    </StrictMode>,
  )
}
