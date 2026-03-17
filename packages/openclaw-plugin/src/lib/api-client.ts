import { ProxyAgent, fetch as undiciFetch } from 'undici'
import { ApiClient as BaseApiClient } from '@actionbookdev/sdk'

function getProxyUrl(): string | undefined {
  return (
    process.env.HTTPS_PROXY ||
    process.env.HTTP_PROXY ||
    process.env.https_proxy ||
    process.env.http_proxy
  )
}

function shouldBypassProxy(url: string): boolean {
  const noProxy = process.env.NO_PROXY || process.env.no_proxy
  if (!noProxy) return false
  if (noProxy === '*') return true

  let urlHost: string
  let urlPort: string
  try {
    const parsed = new URL(url)
    urlHost = parsed.hostname.toLowerCase()
    urlPort = parsed.port || (parsed.protocol === 'https:' ? '443' : '80')
  } catch {
    return false
  }

  const bypassList = noProxy.split(',').map((h) => h.trim().toLowerCase())

  return bypassList.some((bypass) => {
    if (!bypass) return false

    // Handle port-specific entries (e.g. "example.com:8080")
    const [bypassHost, bypassPort] = bypass.includes(':')
      ? [bypass.split(':')[0], bypass.split(':')[1]]
      : [bypass, undefined]

    if (bypassPort && bypassPort !== urlPort) return false

    if (bypassHost.startsWith('.')) {
      return urlHost.endsWith(bypassHost) || urlHost === bypassHost.slice(1)
    }
    return urlHost === bypassHost || urlHost.endsWith('.' + bypassHost)
  })
}

function createProxyFetch(): typeof fetch {
  const proxyUrl = getProxyUrl()
  if (!proxyUrl) {
    return globalThis.fetch
  }

  const proxyAgent = new ProxyAgent(proxyUrl)

  const customFetch = async (
    url: string | URL | Request,
    init?: RequestInit
  ): Promise<Response> => {
    const urlString =
      typeof url === 'string'
        ? url
        : url instanceof URL
        ? url.toString()
        : url.url
    const useProxy = proxyAgent && !shouldBypassProxy(urlString)

    const response = await undiciFetch(
      url as any,
      {
        ...init,
        dispatcher: useProxy ? proxyAgent : undefined,
      } as any
    )

    return response as unknown as Response
  }

  return customFetch as typeof fetch
}

export interface ApiClientOptions {
  apiKey?: string
  timeoutMs?: number
  retry?: {
    maxRetries?: number
    retryDelay?: number
  }
}

export class ApiClient extends BaseApiClient {
  constructor(baseUrl: string, options: ApiClientOptions = {}) {
    super({
      apiKey: options.apiKey || '',
      baseUrl,
      timeoutMs: options.timeoutMs,
      retry: options.retry,
      fetch: createProxyFetch(),
    })
  }
}
