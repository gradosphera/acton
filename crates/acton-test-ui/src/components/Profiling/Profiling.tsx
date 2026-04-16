import type React from "react"
import {Fragment, useEffect, useMemo, useState} from "react"
import {hierarchy, partition, type HierarchyRectangularNode} from "d3-hierarchy"

import type {TestReport} from "@acton/shared-ui"

import styles from "./Profiling.module.css"

export interface ProfilingReport {
  readonly tests: readonly ProfiledTest[]
}

interface ProfiledTest {
  readonly testName: string
  readonly totalGas: number
  readonly totalSamples: number
  readonly functionCount: number
  readonly root: ProfileFrame
  readonly functions: readonly ProfileFunction[]
}

interface ProfileFrame {
  readonly functionName: string
  readonly totalGas: number
  readonly selfGas: number
  readonly sampleCount: number
  readonly url: string
  readonly lineNumber: number
  readonly columnNumber: number
  readonly children: readonly ProfileFrame[]
}

interface ProfileFunction {
  readonly functionName: string
  readonly totalGas: number
  readonly selfGas: number
  readonly sampleCount: number
  readonly url: string
  readonly lineNumber: number
  readonly columnNumber: number
}

interface FlamegraphBreadcrumb {
  readonly label: string
  readonly path: readonly number[]
}

interface FlamegraphNodeDatum extends ProfileFrame {
  readonly path: readonly number[]
  readonly children: readonly FlamegraphNodeDatum[]
}

interface ProfilingProps {
  readonly report: ProfilingReport
  readonly selectedTest?: TestReport
  readonly projectRoot?: string
}

const MAX_FUNCTION_ROWS = 40
const FLAMEGRAPH_WIDTH = 1000
const FLAMEGRAPH_ROW_HEIGHT = 40

const formatGas = (value: number) => `${value.toLocaleString()} gas`

const formatPercent = (value: number) => `${value.toFixed(1)}%`

const getRelativePath = (filePath: string, projectRoot?: string) => {
  if (!filePath) {
    return "No source location"
  }

  if (projectRoot && filePath.startsWith(projectRoot)) {
    const relativePath = filePath.slice(projectRoot.length)
    return relativePath || filePath
  }

  const pathSegments = filePath.split("/")
  if (pathSegments.length > 4) {
    return `.../${pathSegments.slice(-4).join("/")}`
  }

  return filePath
}

const formatLocation = (
  url: string,
  lineNumber: number,
  columnNumber: number,
  projectRoot?: string,
) => {
  if (!url) {
    return "No source location"
  }

  const path = getRelativePath(url, projectRoot)
  if (lineNumber < 0) {
    return path
  }

  return `${path}:${lineNumber + 1}:${Math.max(columnNumber + 1, 1)}`
}

const hashText = (value: string) => {
  let hash = 0
  for (const character of value) {
    hash = (hash * 31 + (character.codePointAt(0) ?? 0)) >>> 0
  }

  return hash
}

const flameColor = (name: string, depth: number) => {
  const hash = hashText(name)
  const hue = hash % 360
  const saturation = 62 - Math.min(depth * 2, 10)
  const lightness = 56 - Math.min(depth, 6) * 3
  return `hsl(${hue} ${saturation}% ${lightness}%)`
}

const getNodeAtPath = (root: ProfileFrame, path: readonly number[]) => {
  let current = root

  for (const index of path) {
    const next = current.children[index]
    if (next === undefined) {
      return root
    }

    current = next
  }

  return current
}

const buildBreadcrumbs = (root: ProfileFrame, path: readonly number[]): FlamegraphBreadcrumb[] => {
  const breadcrumbs: FlamegraphBreadcrumb[] = [{label: "Full test", path: []}]

  if (path.length === 0) {
    return breadcrumbs
  }

  const currentPath: number[] = []
  let currentNode = root

  for (const index of path) {
    currentPath.push(index)
    const next = currentNode.children[index]
    if (next === undefined) {
      break
    }

    breadcrumbs.push({
      label: next.functionName,
      path: [...currentPath],
    })
    currentNode = next
  }

  return breadcrumbs
}

const cloneFrameWithPath = (frame: ProfileFrame, path: readonly number[]): FlamegraphNodeDatum => {
  return {
    ...frame,
    path,
    children: frame.children.map((child, index) => cloneFrameWithPath(child, [...path, index])),
  }
}

const buildFlamegraphLayout = (
  root: ProfileFrame,
  zoomPath: readonly number[],
): HierarchyRectangularNode<FlamegraphNodeDatum> => {
  const zoomedNode = getNodeAtPath(root, zoomPath)

  const flamegraphRoot = hierarchy<FlamegraphNodeDatum>(
    cloneFrameWithPath(zoomedNode, zoomPath),
    node => [...node.children],
  )
    .sum(node => Math.max(node.selfGas, 0))
    .sort((left, right) => (right.value ?? 0) - (left.value ?? 0))

  return partition<FlamegraphNodeDatum>()
    .size([FLAMEGRAPH_WIDTH, (flamegraphRoot.height + 1) * FLAMEGRAPH_ROW_HEIGHT])
    .padding(1)(flamegraphRoot)
}

export const Profiling: React.FC<ProfilingProps> = ({report, selectedTest, projectRoot}) => {
  const [zoomPath, setZoomPath] = useState<readonly number[]>([])

  const profiledTest = useMemo(() => {
    if (selectedTest === undefined) {
      return
    }

    return report.tests.find(test => test.testName === selectedTest.name)
  }, [report.tests, selectedTest])

  useEffect(() => {
    setZoomPath([])
  }, [profiledTest?.testName])

  const breadcrumbs = useMemo(() => {
    if (profiledTest === undefined) {
      return [{label: "Full test", path: []}] satisfies FlamegraphBreadcrumb[]
    }

    return buildBreadcrumbs(profiledTest.root, zoomPath)
  }, [profiledTest, zoomPath])

  const flamegraphRows = useMemo(() => {
    if (profiledTest === undefined || profiledTest.totalGas === 0) {
      return
    }

    return buildFlamegraphLayout(profiledTest.root, zoomPath)
  }, [profiledTest, zoomPath])

  const flamegraphNodes = useMemo(() => {
    if (flamegraphRows === undefined) {
      return []
    }

    return flamegraphRows
      .descendants()
      .filter((node: HierarchyRectangularNode<FlamegraphNodeDatum>) => {
        return node.depth > 0 || node.data.functionName !== "(root)"
      })
  }, [flamegraphRows])

  const flamegraphTotalGas = flamegraphRows?.value ?? profiledTest?.totalGas ?? 0
  const flamegraphHeight = Math.max(
    flamegraphRows?.y1 ?? FLAMEGRAPH_ROW_HEIGHT,
    FLAMEGRAPH_ROW_HEIGHT,
  )

  const topFunctions = useMemo(() => {
    if (profiledTest === undefined) {
      return []
    }

    return profiledTest.functions.slice(0, MAX_FUNCTION_ROWS)
  }, [profiledTest])

  const hottestFunction = topFunctions[0]

  if (selectedTest === undefined) {
    return <div className={styles.emptyState}>Select a test to inspect profiling results.</div>
  }

  if (report.tests.length === 0) {
    return (
      <div className={styles.emptyState}>
        Profiling is enabled, but no executable profiling samples were captured.
      </div>
    )
  }

  if (profiledTest === undefined || profiledTest.totalGas === 0) {
    return (
      <div className={styles.emptyState}>
        No profiling samples were captured for <strong>{selectedTest.name}</strong>. Message
        executions are profiled by default; add <code>--profile-include-tests</code> to include
        getter-based unit tests.
      </div>
    )
  }

  return (
    <div className={styles.profiling}>
      <div className={styles.summaryGrid}>
        <div className={styles.summaryCard}>
          <div className={styles.summaryLabel}>Total Gas</div>
          <div className={styles.summaryValue}>{formatGas(profiledTest.totalGas)}</div>
          <div className={styles.summaryMeta}>Inclusive gas across all profiled samples</div>
        </div>
        <div className={styles.summaryCard}>
          <div className={styles.summaryLabel}>Samples</div>
          <div className={styles.summaryValue}>{profiledTest.totalSamples.toLocaleString()}</div>
          <div className={styles.summaryMeta}>Weighted by VM gas spent per instruction</div>
        </div>
        <div className={styles.summaryCard}>
          <div className={styles.summaryLabel}>Functions</div>
          <div className={styles.summaryValue}>{profiledTest.functionCount.toLocaleString()}</div>
          <div className={styles.summaryMeta}>Unique stack frames observed in this test</div>
        </div>
        <div className={styles.summaryCard}>
          <div className={styles.summaryLabel}>Hottest Frame</div>
          <div className={styles.summaryValueSmall}>
            {hottestFunction?.functionName ?? "No frames"}
          </div>
          <div className={styles.summaryMeta}>
            {hottestFunction ? formatGas(hottestFunction.totalGas) : "No inclusive gas"}
          </div>
        </div>
      </div>

      <div className={styles.panel}>
        <div className={styles.panelHeader}>
          <div>
            <h2 className={styles.panelTitle}>Flamegraph</h2>
            <p className={styles.panelSubtitle}>
              Click a frame to zoom into its subtree. Width shows inclusive gas share within the
              current focus for {selectedTest.name}.
            </p>
          </div>
          <div className={styles.panelMeta}>
            {breadcrumbs.map((crumb, index) => (
              <Fragment key={crumb.path.join("/") || "root"}>
                {index > 0 && <span className={styles.breadcrumbDivider}>/</span>}
                <button
                  type="button"
                  className={`${styles.breadcrumb} ${
                    index === breadcrumbs.length - 1 ? styles.breadcrumbActive : ""
                  }`}
                  onClick={() => setZoomPath(crumb.path)}
                >
                  {crumb.label}
                </button>
              </Fragment>
            ))}
          </div>
        </div>

        <div className={styles.flamegraph}>
          <div className={styles.flamegraphViewport}>
            <svg
              className={styles.flamegraphSvg}
              width={FLAMEGRAPH_WIDTH}
              height={flamegraphHeight}
              viewBox={`0 0 ${FLAMEGRAPH_WIDTH} ${flamegraphHeight}`}
              preserveAspectRatio="none"
              aria-label={`Profiling flamegraph for ${selectedTest.name}`}
              role="img"
            >
              {flamegraphNodes.map(node => {
                const width = Math.max(node.x1 - node.x0, 1)
                const height = Math.max(node.y1 - node.y0, FLAMEGRAPH_ROW_HEIGHT - 4)
                const percent = ((node.value ?? 0) / Math.max(flamegraphTotalGas, 1)) * 100
                const isActive = node.data.path.join("/") === zoomPath.join("/")
                const showLabel = width >= 72
                const showValue = width >= 156

                return (
                  <g
                    key={node.data.path.join("/") || "root"}
                    className={styles.flamegraphNode}
                    role="button"
                    tabIndex={0}
                    aria-label={`${node.data.functionName}, ${formatGas(node.data.totalGas)} inclusive`}
                    transform={`translate(${node.x0}, ${node.y0})`}
                    onClick={() => setZoomPath(node.data.path)}
                    onKeyDown={event => {
                      if (event.key === "Enter" || event.key === " ") {
                        event.preventDefault()
                        setZoomPath(node.data.path)
                      }
                    }}
                  >
                    <title>{`${node.data.functionName}
${formatGas(node.data.totalGas)} inclusive
${formatGas(node.data.selfGas)} self
${formatPercent(percent)} of current focus
${formatLocation(node.data.url, node.data.lineNumber, node.data.columnNumber, projectRoot)}`}</title>
                    <rect
                      className={styles.flamegraphFrame}
                      width={width}
                      height={height}
                      rx={6}
                      ry={6}
                      fill={flameColor(node.data.functionName, node.depth)}
                      opacity={isActive ? 1 : 0.96}
                    />
                    {showLabel && (
                      <text className={styles.flamegraphLabel} x={12} y={height / 2 + 1}>
                        {node.data.functionName}
                      </text>
                    )}
                    {showValue && (
                      <text
                        className={styles.flamegraphValue}
                        x={width - 12}
                        y={height / 2 + 1}
                        textAnchor="end"
                      >
                        {formatPercent(percent)}
                      </text>
                    )}
                  </g>
                )
              })}
            </svg>
          </div>
        </div>
      </div>

      <div className={styles.panel}>
        <div className={styles.panelHeader}>
          <div>
            <h2 className={styles.panelTitle}>Function Table</h2>
            <p className={styles.panelSubtitle}>
              Inclusive gas counts a frame everywhere it appears in the stack. Self gas counts only
              leaf samples.
            </p>
          </div>
        </div>

        <div className={styles.tableWrap}>
          <table className={styles.functionTable}>
            <thead>
              <tr>
                <th>Function</th>
                <th>Inclusive Gas</th>
                <th>Inclusive %</th>
                <th>Self Gas</th>
                <th>Samples</th>
                <th>Location</th>
              </tr>
            </thead>
            <tbody>
              {topFunctions.map(frame => (
                <tr
                  key={`${frame.functionName}:${frame.url}:${frame.lineNumber}:${frame.columnNumber}`}
                >
                  <td>
                    <div className={styles.functionName}>{frame.functionName}</div>
                  </td>
                  <td>{formatGas(frame.totalGas)}</td>
                  <td>{formatPercent((frame.totalGas / profiledTest.totalGas) * 100)}</td>
                  <td>{formatGas(frame.selfGas)}</td>
                  <td>{frame.sampleCount.toLocaleString()}</td>
                  <td className={styles.locationCell}>
                    {formatLocation(frame.url, frame.lineNumber, frame.columnNumber, projectRoot)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  )
}
