export type Severity = "error" | "warning" | "warn" | "info" | "help" | "fatal";

export interface Point {
  row: number;
  column: number;
}

export interface PluginDiagnostic {
  message: string;
  severity?: Severity;
  help?: string;
  start?: number;
  end?: number;
  span?: {
    start: number;
    end: number;
  };
}

export interface RawCstNode {
  kind: string;
  fieldName?: string;
  named: boolean;
  startByte: number;
  endByte: number;
  startPosition: Point;
  endPosition: Point;
  hasError: boolean;
  isError: boolean;
  isMissing: boolean;
  children: RawCstNode[];
}

export interface SyntaxNode {
  readonly type: string;
  readonly kind: string;
  readonly isNamed: boolean;
  readonly hasError: boolean;
  readonly isError: boolean;
  readonly isMissing: boolean;
  readonly startIndex: number;
  readonly endIndex: number;
  readonly startByte: number;
  readonly endByte: number;
  readonly startPosition: Point;
  readonly endPosition: Point;
  readonly text: string;
  readonly parent: SyntaxNode | null;
  readonly fieldName: string | null;
  readonly children: SyntaxNode[];
  readonly namedChildren: SyntaxNode[];
  readonly childCount: number;
  readonly namedChildCount: number;
  readonly firstChild: SyntaxNode | null;
  readonly lastChild: SyntaxNode | null;
  readonly firstNamedChild: SyntaxNode | null;
  readonly lastNamedChild: SyntaxNode | null;
  readonly nextSibling: SyntaxNode | null;
  readonly previousSibling: SyntaxNode | null;
  readonly nextNamedSibling: SyntaxNode | null;
  readonly previousNamedSibling: SyntaxNode | null;

  child(index: number): SyntaxNode | null;
  namedChild(index: number): SyntaxNode | null;
  childForFieldName(fieldName: string): SyntaxNode | null;
  childrenForFieldName(fieldName: string): SyntaxNode[];
  descendantForIndex(startIndex: number, endIndex?: number): SyntaxNode | null;
  namedDescendantForIndex(startIndex: number, endIndex?: number): SyntaxNode | null;
  descendantForPosition(startPosition: Point, endPosition?: Point): SyntaxNode | null;
  namedDescendantForPosition(startPosition: Point, endPosition?: Point): SyntaxNode | null;
  descendantsOfType(
    types: string | string[],
    startPosition?: Point,
    endPosition?: Point,
  ): SyntaxNode[];
}

export interface Tree {
  rootNode: SyntaxNode;
}

export interface LintContext {
  filePath: string;
  source: string;
  cst: RawCstNode;
  tree: Tree;
  rootNode: SyntaxNode;
}

export type LintPluginResult =
  | PluginDiagnostic[]
  | Promise<PluginDiagnostic[]>
  | null
  | undefined;

export type LintPlugin = (ctx: LintContext) => LintPluginResult;
