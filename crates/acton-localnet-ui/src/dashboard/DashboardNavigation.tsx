import {
  ArrowUpRight,
  BookOpen,
  Boxes,
  Braces,
  FileJson,
  Github,
  Image,
  KeyRound,
  LayoutGrid,
  Menu,
  Moon,
  Search as SearchIcon,
  Settings2,
  Sun,
  Wallet,
  X,
} from "lucide-react"
import type {LucideIcon} from "lucide-react"
import * as React from "react"
import {useLocation, useNavigate} from "react-router-dom"

import type {TonClient} from "../explorer/api/client"
import {readExplorerLastPath, writeExplorerLastPath} from "../explorer/explorerResume"
import {useNetworkInfo} from "../explorer/hooks/useNetworkInfo"

import {DashboardSearch} from "./DashboardSearch"
import styles from "./DashboardPage.module.css"

interface DashboardNavigationProps {
  readonly client: TonClient
  readonly theme: string
  readonly setTheme: (theme: string) => void
}

interface SidebarItem {
  readonly label: string
  readonly icon: LucideIcon
  readonly path?: string
  readonly href?: string
}

const mainItems: SidebarItem[] = [
  {label: "Home", icon: LayoutGrid, path: "/dashboard"},
  {label: "Explorer", icon: SearchIcon, path: "/explorer"},
  {label: "Wallets", icon: KeyRound, path: "/wallets"},
  {label: "Faucet", icon: Wallet, path: "/faucet"},
  {label: "Tokens", icon: Boxes, path: "/tokens"},
  {label: "NFTs", icon: Image, path: "/nfts"},
]

const apiItems: SidebarItem[] = [
  {label: "v2 API", icon: FileJson, path: "/api-reference/v2"},
  {label: "v3 API", icon: Braces, path: "/api-reference/v3"},
  {label: "Control API", icon: Settings2, path: "/api-reference/control"},
]

const navigationSections: Array<{readonly id: string; readonly items: readonly SidebarItem[]}> = [
  {id: "main", items: mainItems},
  {id: "api", items: apiItems},
]

const footerItems: SidebarItem[] = [
  {
    label: "Documentation",
    icon: BookOpen,
    href: "https://ton-blockchain.github.io/acton/docs/welcome",
  },
  {label: "GitHub", icon: Github, href: "https://github.com/ton-blockchain/acton"},
]

export const DashboardNavigation: React.FC<DashboardNavigationProps> = ({
  client,
  theme,
  setTheme,
}) => {
  const location = useLocation()
  const navigate = useNavigate()
  const {forkNetwork} = useNetworkInfo()
  const [explorerPath, setExplorerPath] = React.useState(() => readExplorerLastPath())
  const [mobileMenuOpen, setMobileMenuOpen] = React.useState(false)
  const forkBadgeLabel = React.useMemo(() => formatForkNetworkLabel(forkNetwork), [forkNetwork])
  const closeMobileMenu = React.useCallback(() => setMobileMenuOpen(false), [])

  React.useEffect(() => {
    if (!location.pathname.startsWith("/explorer")) {
      return
    }

    const nextPath = `${location.pathname}${location.search}${location.hash}`
    writeExplorerLastPath(nextPath)
    setExplorerPath(nextPath)
  }, [location.hash, location.pathname, location.search])

  React.useEffect(() => {
    closeMobileMenu()
  }, [closeMobileMenu, location.hash, location.pathname, location.search])

  React.useEffect(() => {
    if (!mobileMenuOpen) {
      return
    }

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        closeMobileMenu()
      }
    }

    globalThis.addEventListener("keydown", onKeyDown)
    return () => globalThis.removeEventListener("keydown", onKeyDown)
  }, [closeMobileMenu, mobileMenuOpen])

  const renderWorkspaceHeader = () => (
    <div className={styles.workspaceHeader}>
      <span className={styles.workspaceMark} />
      <span className={styles.workspaceBody}>
        <span className={styles.workspaceTitleRow}>
          <span className={styles.workspaceName}>TON Localnet</span>
          {forkBadgeLabel && <span className={styles.workspaceForkBadge}>{forkBadgeLabel}</span>}
        </span>
        <span className={styles.workspaceMeta}>by Acton</span>
      </span>
    </div>
  )

  return (
    <>
      <header className={styles.mobileTopbar}>
        {renderWorkspaceHeader()}
        <button
          type="button"
          className={styles.mobileMenuButton}
          aria-label="Open navigation menu"
          aria-expanded={mobileMenuOpen}
          onClick={() => setMobileMenuOpen(true)}
        >
          <Menu size={20} />
        </button>
      </header>

      <button
        type="button"
        className={`${styles.mobileBackdrop} ${mobileMenuOpen ? styles.mobileBackdropOpen : ""}`}
        aria-label="Close navigation menu"
        tabIndex={mobileMenuOpen ? 0 : -1}
        onClick={closeMobileMenu}
      />

      <aside
        className={`${styles.sidebar} ${mobileMenuOpen ? styles.sidebarOpen : ""}`}
        aria-label="Main navigation"
      >
        <div className={styles.sidebarHeader}>
          {renderWorkspaceHeader()}
          <button
            type="button"
            className={styles.mobileCloseButton}
            aria-label="Close navigation menu"
            onClick={closeMobileMenu}
          >
            <X size={20} />
          </button>
        </div>

        <div className={styles.topControls}>
          <DashboardSearch client={client} />
        </div>

        <div className={styles.navScroll}>
          <nav className={styles.nav}>
            {navigationSections.map((section, index) => (
              <React.Fragment key={section.id}>
                {index > 0 && <div className={styles.navDivider} />}
                <div className={styles.navSection}>
                  {section.items.map(item => {
                    const Icon = item.icon
                    const targetPath = item.path === "/explorer" ? explorerPath : item.path
                    const isActive =
                      item.path === "/explorer"
                        ? location.pathname.startsWith("/explorer")
                        : item.path === location.pathname

                    return (
                      <button
                        type="button"
                        key={item.label}
                        className={`${styles.navItem} ${isActive ? styles.navItemActive : ""}`}
                        onClick={() => {
                          if (targetPath) {
                            void navigate(targetPath)
                          }
                          closeMobileMenu()
                        }}
                      >
                        <span className={styles.navItemMain}>
                          <Icon size={18} />
                          <span>{item.label}</span>
                        </span>
                      </button>
                    )
                  })}
                </div>
              </React.Fragment>
            ))}

            <div className={styles.navDivider} />

            <div className={styles.navFooter}>
              <div className={styles.navSection}>
                {footerItems.map(item => {
                  const Icon = item.icon

                  return (
                    <a
                      key={item.label}
                      className={styles.navItem}
                      href={item.href}
                      target="_blank"
                      rel="noreferrer"
                      onClick={closeMobileMenu}
                    >
                      <span className={styles.navItemMain}>
                        <Icon size={18} />
                        <span>{item.label}</span>
                      </span>
                      <ArrowUpRight size={14} />
                    </a>
                  )
                })}
              </div>

              <button
                type="button"
                className={styles.themeSwitch}
                aria-label="Toggle Theme"
                data-theme-toggle=""
                onClick={() => setTheme(theme === "light" ? "dark" : "light")}
              >
                <Sun
                  fill="currentColor"
                  className={`${styles.themeSwitchItem} ${theme === "light" ? styles.themeSwitchItemActive : ""}`}
                />
                <Moon
                  fill="currentColor"
                  className={`${styles.themeSwitchItem} ${theme === "dark" ? styles.themeSwitchItemActive : ""}`}
                />
              </button>
            </div>
          </nav>
        </div>
      </aside>
    </>
  )
}

function formatForkNetworkLabel(forkNetwork?: string): string | undefined {
  const normalizedForkNetwork = forkNetwork?.trim()
  if (!normalizedForkNetwork) {
    return undefined
  }

  return `${normalizedForkNetwork.toLocaleLowerCase()} fork`
}
