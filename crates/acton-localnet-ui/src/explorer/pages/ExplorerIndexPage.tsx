import type {FC} from "react"

import {ExplorerSearch} from "../components/ExplorerSearch"

import styles from "./ExplorerIndexPage.module.css"

export const ExplorerIndexPage: FC = () => {
  return (
    <div className={styles.inputPage}>
      <div className={styles.centeredInputContainer}>
        <header className={styles.logoSection}>
          <h1 className={styles.logoTitle}>Explore any address</h1>
        </header>

        <ExplorerSearch autoFocus />
      </div>
    </div>
  )
}
