import { beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('axios', async (importOriginal) => {
  const actual: any = await importOriginal()
  return {
    ...actual,
    default: {
      ...actual.default,
      create: vi.fn()
    }
  }
})

describe('http interceptor single-flight refresh', () => {
  let requestHandlers: Array<(config: any) => any> = []
  let responseHandlers: Array<{
    fulfilled: (res: any) => any
    rejected: (err: any) => any
  }> = []

  beforeEach(async () => {
    requestHandlers = []
    responseHandlers = []
    vi.resetModules()
    const axios = (await import('axios')).default
    // Axios instances are callable functions with .post/.get/etc attached.
    const instance = vi.fn().mockResolvedValue({ data: 'ok' }) as any
    instance.post = vi.fn().mockResolvedValue({ data: 'ok' })
    instance.get = vi.fn().mockResolvedValue({ data: {} })
    instance.interceptors = {
      request: { use: (h: any) => requestHandlers.push(h) },
      response: { use: (f: any, r: any) => responseHandlers.push({ fulfilled: f, rejected: r }) }
    }
    instance.defaults = {}
    vi.mocked(axios.create).mockReturnValue(instance)
  })

  async function loadHttp() {
    const mod = await import('@/api/http')
    return mod
  }

  it('attaches access token from getter to request headers', async () => {
    const { registerAuthAccessors } = await loadHttp()
    const getter = vi.fn().mockReturnValue('my-token')
    registerAuthAccessors(getter, vi.fn())

    const config: { headers: Record<string, string> } = { headers: {} }
    for (const h of requestHandlers) {
      await h(config)
    }
    expect(config.headers.Authorization).toBe('Bearer my-token')
  })

  it('passes through non-401 errors', async () => {
    const { registerAuthAccessors } = await loadHttp()
    registerAuthAccessors(() => null, vi.fn())

    const err = { response: { status: 500 }, config: {} }
    await expect(responseHandlers[0].rejected(err)).rejects.toEqual(err)
  })

  it('triggers single refresh on first 401 and retries the request', async () => {
    const { registerAuthAccessors } = await loadHttp()
    const refresh = vi.fn().mockResolvedValue({ accessToken: 'new-tok', exp: 9999999999 })
    registerAuthAccessors(() => 'old-tok', refresh)

    const err = {
      response: { status: 401 },
      config: { headers: {}, url: '/graphql', _retried: undefined }
    }

    const result = await responseHandlers[0].rejected(err)
    expect(refresh).toHaveBeenCalledTimes(1)
    expect(result).toEqual({ data: 'ok' })
  })

  it('queues concurrent 401s behind a single in-flight refresh', async () => {
    const { default: http, registerAuthAccessors } = await loadHttp()
    let resolveRefresh!: (v: any) => void
    const refresh = vi.fn().mockImplementation(
      () => new Promise((resolve) => { resolveRefresh = resolve })
    )
    registerAuthAccessors(() => 'old-tok', refresh)

    const err1 = { response: { status: 401 }, config: { headers: {}, _retried: undefined } }
    const err2 = { response: { status: 401 }, config: { headers: {}, _retried: undefined } }

    const p1 = responseHandlers[0].rejected(err1)
    const p2 = responseHandlers[0].rejected(err2)

    expect(refresh).toHaveBeenCalledTimes(1)

    resolveRefresh({ accessToken: 'shared-tok', exp: 9999999999 })
    await Promise.all([p1, p2])

    // The retry calls http(originalRequest) (instance as function) for each.
    expect((http as unknown as ReturnType<typeof vi.fn>)).toHaveBeenCalledTimes(2)
  })
})
