import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// Mock undici before importing the module under test
const mockUndiciFetch = vi.fn();
const MockProxyAgent = vi.fn();

vi.mock("undici", () => ({
  fetch: mockUndiciFetch,
  ProxyAgent: MockProxyAgent,
}));

// Mock @actionbookdev/sdk
const MockBaseApiClient = vi.fn();
vi.mock("@actionbookdev/sdk", () => ({
  ApiClient: MockBaseApiClient,
}));

// Dynamic import to pick up mocks
const { ApiClient } = await import("./api-client.js");

describe("ApiClient", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    delete process.env.HTTPS_PROXY;
    delete process.env.HTTP_PROXY;
    delete process.env.https_proxy;
    delete process.env.http_proxy;
    delete process.env.NO_PROXY;
    delete process.env.no_proxy;
  });

  afterEach(() => {
    delete process.env.HTTPS_PROXY;
    delete process.env.HTTP_PROXY;
    delete process.env.https_proxy;
    delete process.env.http_proxy;
    delete process.env.NO_PROXY;
    delete process.env.no_proxy;
  });

  it("passes options to base SDK client", () => {
    new ApiClient("https://api.actionbook.dev", {
      apiKey: "test-key",
      timeoutMs: 5000,
      retry: { maxRetries: 2, retryDelay: 500 },
    });

    expect(MockBaseApiClient).toHaveBeenCalledWith(
      expect.objectContaining({
        apiKey: "test-key",
        baseUrl: "https://api.actionbook.dev",
        timeoutMs: 5000,
        retry: { maxRetries: 2, retryDelay: 500 },
        fetch: expect.any(Function),
      })
    );
  });

  it("uses globalThis.fetch when no proxy is set", () => {
    new ApiClient("https://api.actionbook.dev");

    const passedOptions = MockBaseApiClient.mock.calls[0][0];
    // When no proxy env, should use globalThis.fetch
    expect(passedOptions.fetch).toBe(globalThis.fetch);
  });

  it("defaults apiKey to empty string", () => {
    new ApiClient("https://api.actionbook.dev");

    expect(MockBaseApiClient).toHaveBeenCalledWith(
      expect.objectContaining({ apiKey: "" })
    );
  });
});

describe("proxy fetch behavior", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    delete process.env.HTTPS_PROXY;
    delete process.env.HTTP_PROXY;
    delete process.env.https_proxy;
    delete process.env.http_proxy;
    delete process.env.NO_PROXY;
    delete process.env.no_proxy;
  });

  afterEach(() => {
    delete process.env.HTTPS_PROXY;
    delete process.env.HTTP_PROXY;
    delete process.env.https_proxy;
    delete process.env.http_proxy;
    delete process.env.NO_PROXY;
    delete process.env.no_proxy;
  });

  it("creates ProxyAgent and uses undici fetch when HTTPS_PROXY is set", async () => {
    process.env.HTTPS_PROXY = "http://proxy.corp:8080";
    mockUndiciFetch.mockResolvedValue({ ok: true, status: 200 });

    // Re-import to pick up env change — need fresh module
    vi.resetModules();
    vi.doMock("undici", () => ({
      fetch: mockUndiciFetch,
      ProxyAgent: MockProxyAgent,
    }));
    vi.doMock("@actionbookdev/sdk", () => ({
      ApiClient: MockBaseApiClient,
    }));
    const mod = await import("./api-client.js");
    new mod.ApiClient("https://api.actionbook.dev");

    const passedOptions = MockBaseApiClient.mock.calls[0][0];
    expect(passedOptions.fetch).not.toBe(globalThis.fetch);
    expect(MockProxyAgent).toHaveBeenCalledWith("http://proxy.corp:8080");

    // Call the custom fetch to verify it uses undici
    await passedOptions.fetch("https://api.actionbook.dev/v1/search");
    expect(mockUndiciFetch).toHaveBeenCalledWith(
      "https://api.actionbook.dev/v1/search",
      expect.objectContaining({
        dispatcher: expect.anything(),
      })
    );
  });

  it("bypasses proxy when NO_PROXY=*", async () => {
    process.env.HTTPS_PROXY = "http://proxy.corp:8080";
    process.env.NO_PROXY = "*";
    mockUndiciFetch.mockResolvedValue({ ok: true, status: 200 });

    vi.resetModules();
    vi.doMock("undici", () => ({
      fetch: mockUndiciFetch,
      ProxyAgent: MockProxyAgent,
    }));
    vi.doMock("@actionbookdev/sdk", () => ({
      ApiClient: MockBaseApiClient,
    }));
    const mod = await import("./api-client.js");
    new mod.ApiClient("https://api.actionbook.dev");

    const passedOptions = MockBaseApiClient.mock.calls[0][0];
    await passedOptions.fetch("https://api.actionbook.dev/v1/search");

    expect(mockUndiciFetch).toHaveBeenCalledWith(
      "https://api.actionbook.dev/v1/search",
      expect.objectContaining({
        dispatcher: undefined,
      })
    );
  });

  it("bypasses proxy for matching NO_PROXY domain", async () => {
    process.env.HTTPS_PROXY = "http://proxy.corp:8080";
    process.env.NO_PROXY = "api.actionbook.dev,.internal.corp";
    mockUndiciFetch.mockResolvedValue({ ok: true, status: 200 });

    vi.resetModules();
    vi.doMock("undici", () => ({
      fetch: mockUndiciFetch,
      ProxyAgent: MockProxyAgent,
    }));
    vi.doMock("@actionbookdev/sdk", () => ({
      ApiClient: MockBaseApiClient,
    }));
    const mod = await import("./api-client.js");
    new mod.ApiClient("https://api.actionbook.dev");

    const passedOptions = MockBaseApiClient.mock.calls[0][0];

    // Exact match — should bypass
    await passedOptions.fetch("https://api.actionbook.dev/v1/search");
    expect(mockUndiciFetch).toHaveBeenLastCalledWith(
      "https://api.actionbook.dev/v1/search",
      expect.objectContaining({ dispatcher: undefined })
    );

    // Suffix match — should bypass
    await passedOptions.fetch("https://foo.internal.corp/api");
    expect(mockUndiciFetch).toHaveBeenLastCalledWith(
      "https://foo.internal.corp/api",
      expect.objectContaining({ dispatcher: undefined })
    );
  });

  it("uses proxy for non-matching domains", async () => {
    process.env.HTTPS_PROXY = "http://proxy.corp:8080";
    process.env.NO_PROXY = "internal.corp";
    mockUndiciFetch.mockResolvedValue({ ok: true, status: 200 });

    vi.resetModules();
    vi.doMock("undici", () => ({
      fetch: mockUndiciFetch,
      ProxyAgent: MockProxyAgent,
    }));
    vi.doMock("@actionbookdev/sdk", () => ({
      ApiClient: MockBaseApiClient,
    }));
    const mod = await import("./api-client.js");
    new mod.ApiClient("https://api.actionbook.dev");

    const passedOptions = MockBaseApiClient.mock.calls[0][0];
    await passedOptions.fetch("https://api.actionbook.dev/v1/search");

    expect(mockUndiciFetch).toHaveBeenCalledWith(
      "https://api.actionbook.dev/v1/search",
      expect.objectContaining({
        dispatcher: expect.anything(),
      })
    );
    // dispatcher should NOT be undefined (proxy is used)
    const call = mockUndiciFetch.mock.calls[0][1];
    expect(call.dispatcher).not.toBeUndefined();
  });

  it("handles URL object input", async () => {
    process.env.HTTPS_PROXY = "http://proxy.corp:8080";
    mockUndiciFetch.mockResolvedValue({ ok: true, status: 200 });

    vi.resetModules();
    vi.doMock("undici", () => ({
      fetch: mockUndiciFetch,
      ProxyAgent: MockProxyAgent,
    }));
    vi.doMock("@actionbookdev/sdk", () => ({
      ApiClient: MockBaseApiClient,
    }));
    const mod = await import("./api-client.js");
    new mod.ApiClient("https://api.actionbook.dev");

    const passedOptions = MockBaseApiClient.mock.calls[0][0];
    const urlObj = new URL("https://api.actionbook.dev/v1/search");
    await passedOptions.fetch(urlObj);

    expect(mockUndiciFetch).toHaveBeenCalled();
  });

  it("reads HTTP_PROXY as fallback", async () => {
    process.env.HTTP_PROXY = "http://fallback-proxy:3128";
    mockUndiciFetch.mockResolvedValue({ ok: true, status: 200 });

    vi.resetModules();
    vi.doMock("undici", () => ({
      fetch: mockUndiciFetch,
      ProxyAgent: MockProxyAgent,
    }));
    vi.doMock("@actionbookdev/sdk", () => ({
      ApiClient: MockBaseApiClient,
    }));
    const mod = await import("./api-client.js");
    new mod.ApiClient("https://api.actionbook.dev");

    expect(MockProxyAgent).toHaveBeenCalledWith("http://fallback-proxy:3128");
  });
});
