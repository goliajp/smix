import { Server } from '@modelcontextprotocol/sdk/server/index.js'
import {
  CallToolRequestSchema,
  ListToolsRequestSchema,
} from '@modelcontextprotocol/sdk/types.js'
import { SimctlClient } from '../sim/simctl.js'
import {
  ALL_TOOLS,
  type AcquireDriverFn,
  type ToolCallResult,
  type ToolContext,
  type ToolDef,
} from './tools.js'
import { defaultAcquireDriver } from './driver-factory.js'

export type Logger = (msg: string) => void

const defaultLogger: Logger = (msg) =>
  process.stderr.write(`[simx-mcp] ${msg}\n`)

export type CreateSimxMcpServerOptions = {
  name?: string
  version?: string
  log?: Logger
  client?: SimctlClient
  acquireDriver?: AcquireDriverFn
}

function toolToListEntry(tool: ToolDef): {
  name: string
  description: string
  inputSchema: ToolDef['inputJsonSchema']
  outputSchema?: ToolDef['outputJsonSchema']
} {
  const entry: {
    name: string
    description: string
    inputSchema: ToolDef['inputJsonSchema']
    outputSchema?: ToolDef['outputJsonSchema']
  } = {
    name: tool.name,
    description: tool.description,
    inputSchema: tool.inputJsonSchema,
  }
  if (tool.outputJsonSchema !== undefined) {
    entry.outputSchema = tool.outputJsonSchema
  }
  return entry
}

export function createSimxMcpServer(
  opts: CreateSimxMcpServerOptions = {},
): Server {
  const name = opts.name ?? 'simx'
  const version = opts.version ?? '0.0.0'
  const log = opts.log ?? defaultLogger
  const client = opts.client ?? new SimctlClient()
  const acquireDriver = opts.acquireDriver ?? defaultAcquireDriver(client)
  const ctx: ToolContext = { client, acquireDriver }

  const server = new Server(
    { name, version },
    { capabilities: { tools: {} } },
  )

  server.setRequestHandler(ListToolsRequestSchema, async () => {
    log('tools/list requested')
    return { tools: ALL_TOOLS.map(toolToListEntry) }
  })

  server.setRequestHandler(CallToolRequestSchema, async (req): Promise<ToolCallResult> => {
    const tool = ALL_TOOLS.find((t) => t.name === req.params.name)
    if (tool === undefined) {
      log(`tools/call unknown tool: ${req.params.name}`)
      return {
        content: [
          { type: 'text', text: `tool not found: ${req.params.name}` },
        ],
        isError: true,
      }
    }
    const parsed = tool.inputSchema.safeParse(req.params.arguments ?? {})
    if (!parsed.success) {
      log(`tools/call invalid args for ${tool.name}: ${parsed.error.message}`)
      return {
        content: [
          { type: 'text', text: `invalid arguments: ${parsed.error.message}` },
        ],
        isError: true,
      }
    }
    return tool.handler(parsed.data, ctx)
  })

  log(`server created: ${name}@${version}`)
  return server
}
