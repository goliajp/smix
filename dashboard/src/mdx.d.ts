declare module '*.mdx' {
  const Component: import('react').ComponentType
  export default Component
  export const frontmatter: Record<string, unknown> | undefined
}
