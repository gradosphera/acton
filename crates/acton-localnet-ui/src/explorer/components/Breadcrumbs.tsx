import {ChevronRight} from "lucide-react"
import {Link} from "react-router-dom"
import {Fragment} from "react"
import type {FC} from "react"

import {AddressLabel} from "./AddressLabel"
import styles from "./Breadcrumbs.module.css"
import {formatAddress} from "./utils"

export interface BreadcrumbItem {
  readonly label: string
  readonly path?: string
  readonly isAddress?: boolean
  readonly isHash?: boolean
}

interface BreadcrumbsProps {
  readonly items: BreadcrumbItem[]
}

export const Breadcrumbs: FC<BreadcrumbsProps> = ({items}) => {
  const formatItem = (item: BreadcrumbItem) => {
    if (item.isAddress) {
      return <AddressLabel address={item.label} />
    }
    if (item.isHash) {
      return formatAddress(item.label)
    }
    return item.label
  }

  return (
    <nav className={styles.breadcrumbs}>
      <Link to="/explorer" className={styles.item}>
        <span>Explore</span>
      </Link>
      {items.map((item, index) => {
        const key = `${item.label}-${index}`
        return (
          <Fragment key={key}>
            <ChevronRight size={14} className={styles.separator} />
            {item.path ? (
              <Link to={item.path} className={styles.item}>
                {formatItem(item)}
              </Link>
            ) : (
              <span className={`${styles.item} ${styles.current}`}>{formatItem(item)}</span>
            )}
          </Fragment>
        )
      })}
    </nav>
  )
}
