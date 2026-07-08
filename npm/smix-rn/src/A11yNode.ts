// v7.5 c1 — A11yNode + Rect mirror Rust smix-screen A11yNode +
// Swift v7.2 + Kotlin v7.4 A11yNode. CamelCase wire keys match
// Rust `#[serde(rename_all = "camelCase")]`.

export interface Rect {
  x: number
  y: number
  w: number
  h: number
}

export function rectCenter(r: Rect): { x: number; y: number } {
  return { x: r.x + r.w / 2, y: r.y + r.h / 2 }
}

export type A11yRole =
  | 'button' | 'link' | 'textField' | 'secureTextField' | 'searchField'
  | 'switch' | 'toggle' | 'checkBox' | 'radio' | 'image' | 'staticText'
  | 'tab' | 'tabBar' | 'navigationBar' | 'cell' | 'alert' | 'dialog'
  | 'slider' | 'progressBar' | 'picker' | 'menu' | 'menuItem'
  | 'scrollView' | 'segmentedControl' | 'table' | 'collectionView'
  | 'webView' | 'keyboard'

export interface A11yNode {
  rawType: string
  role?: A11yRole | undefined
  identifier?: string | undefined
  label?: string | undefined
  title?: string | undefined
  placeholderValue?: string | undefined
  value?: string | undefined
  text?: string | undefined
  bounds: Rect
  enabled?: boolean | undefined
  selected?: boolean | undefined
  hasFocus?: boolean | undefined
  visible?: boolean | undefined
  children?: A11yNode[] | undefined
}

/**
 * Recursively find the first node whose [identifier] matches [id].
 */
export function findById(node: A11yNode, id: string): A11yNode | null {
  if (node.identifier === id) return node
  for (const child of node.children ?? []) {
    const hit = findById(child, id)
    if (hit !== null) return hit
  }
  return null
}

/**
 * DFS pre-order flatten — self + all descendants in a single list.
 */
export function flatten(node: A11yNode): A11yNode[] {
  const out: A11yNode[] = [node]
  for (const child of node.children ?? []) {
    out.push(...flatten(child))
  }
  return out
}
