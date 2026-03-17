import { describe, expect, it, vi } from "vitest";
import type { OpenClawPluginApi } from "openclaw/plugin-sdk/core";
import { ActionbookError, ErrorCodes } from "@actionbookdev/sdk";
import plugin from "./plugin.js";

// Mock the ApiClient so we can control SDK responses
const mockSearchActions = vi.fn();
const mockGetActionByAreaId = vi.fn();

vi.mock("./lib/api-client.js", () => ({
  ApiClient: vi.fn().mockImplementation(() => ({
    searchActions: mockSearchActions,
    getActionByAreaId: mockGetActionByAreaId,
  })),
}));

function createTestPluginApi(
  overrides: Partial<OpenClawPluginApi> = {}
): OpenClawPluginApi {
  return {
    id: "actionbook",
    name: "Actionbook",
    description: "Actionbook",
    source: "test",
    config: {} as never,
    pluginConfig: {},
    runtime: {} as never,
    logger: {
      debug: vi.fn(),
      info: vi.fn(),
      warn: vi.fn(),
      error: vi.fn(),
    },
    registerTool: vi.fn(),
    registerHook: vi.fn(),
    registerHttpRoute: vi.fn(),
    registerChannel: vi.fn(),
    registerGatewayMethod: vi.fn(),
    registerCli: vi.fn(),
    registerService: vi.fn(),
    registerProvider: vi.fn(),
    registerCommand: vi.fn(),
    registerContextEngine: vi.fn(),
    resolvePath(input: string) {
      return input;
    },
    on: vi.fn(),
    ...overrides,
  };
}

function registerAndCapture(
  overrides: Partial<OpenClawPluginApi> = {}
): {
  tools: Map<string, { execute: (id: string, params: unknown) => Promise<unknown> }>;
  api: OpenClawPluginApi;
} {
  const tools = new Map<string, { execute: (id: string, params: unknown) => Promise<unknown> }>();
  const registerTool = vi.fn().mockImplementation((tool: { name: string }) => {
    tools.set(tool.name, tool as never);
  });
  const api = createTestPluginApi({ registerTool, ...overrides });
  plugin.register(api);
  return { tools, api };
}

describe("plugin registration", () => {
  it("registers tools and prompt guidance without info log noise", async () => {
    const registerTool = vi.fn();
    const on = vi.fn();
    const logger = {
      debug: vi.fn(),
      info: vi.fn(),
      warn: vi.fn(),
      error: vi.fn(),
    };

    plugin.register(
      createTestPluginApi({
        registerTool,
        on,
        logger,
      })
    );

    expect(logger.info).not.toHaveBeenCalled();
    expect(on).toHaveBeenCalledTimes(1);
    expect(on.mock.calls[0]?.[0]).toBe("before_prompt_build");

    const beforePromptBuild = on.mock.calls[0]?.[1];
    const result = await beforePromptBuild?.({}, {});
    expect(result).toMatchObject({
      prependSystemContext: expect.stringContaining("Actionbook Browser Automation"),
    });

    expect(registerTool).toHaveBeenCalledTimes(2);
    expect(registerTool.mock.calls.map(([tool]) => tool.name)).toEqual([
      "search_actions",
      "get_action_by_area_id",
    ]);
    expect(registerTool.mock.calls.every(([, opts]) => opts === undefined)).toBe(
      true
    );
  });

  it("validates apiUrl at registration time", () => {
    const logger = { debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() };
    const registerTool = vi.fn();
    plugin.register(
      createTestPluginApi({
        pluginConfig: { apiUrl: "not-a-url" },
        logger,
        registerTool,
      })
    );
    expect(logger.error).toHaveBeenCalledWith(
      expect.stringContaining('invalid apiUrl "not-a-url"')
    );
    expect(registerTool).not.toHaveBeenCalled();
  });
});

// ============================================================================
// Critical #4: SSRF protection in resolveApiUrl
// ============================================================================

describe("resolveApiUrl SSRF protection", () => {
  it("rejects localhost", () => {
    const logger = { debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() };
    const registerTool = vi.fn();
    plugin.register(
      createTestPluginApi({
        pluginConfig: { apiUrl: "http://localhost:3000" },
        logger,
        registerTool,
      })
    );
    expect(logger.error).toHaveBeenCalledWith(
      expect.stringContaining("private/local addresses")
    );
    expect(registerTool).not.toHaveBeenCalled();
  });

  it("rejects 127.0.0.1", () => {
    const logger = { debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() };
    const registerTool = vi.fn();
    plugin.register(
      createTestPluginApi({
        pluginConfig: { apiUrl: "http://127.0.0.1:8080" },
        logger,
        registerTool,
      })
    );
    expect(logger.error).toHaveBeenCalledWith(
      expect.stringContaining("private/local addresses")
    );
    expect(registerTool).not.toHaveBeenCalled();
  });

  it("rejects 10.x private range", () => {
    const logger = { debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() };
    const registerTool = vi.fn();
    plugin.register(
      createTestPluginApi({
        pluginConfig: { apiUrl: "http://10.0.0.1/api" },
        logger,
        registerTool,
      })
    );
    expect(logger.error).toHaveBeenCalledWith(
      expect.stringContaining("private/local addresses")
    );
    expect(registerTool).not.toHaveBeenCalled();
  });

  it("rejects 192.168.x", () => {
    const logger = { debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() };
    const registerTool = vi.fn();
    plugin.register(
      createTestPluginApi({
        pluginConfig: { apiUrl: "http://192.168.1.1" },
        logger,
        registerTool,
      })
    );
    expect(logger.error).toHaveBeenCalledWith(
      expect.stringContaining("private/local addresses")
    );
  });

  it("rejects 169.254.x (link-local / cloud metadata)", () => {
    const logger = { debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() };
    const registerTool = vi.fn();
    plugin.register(
      createTestPluginApi({
        pluginConfig: { apiUrl: "http://169.254.169.254/latest/meta-data" },
        logger,
        registerTool,
      })
    );
    expect(logger.error).toHaveBeenCalledWith(
      expect.stringContaining("private/local addresses")
    );
  });

  it("rejects bracketed private IPv6 (e.g. [fd00::1])", () => {
    const logger = { debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() };
    const registerTool = vi.fn();
    plugin.register(
      createTestPluginApi({
        pluginConfig: { apiUrl: "http://[fd00::1]:8080/api" },
        logger,
        registerTool,
      })
    );
    expect(logger.error).toHaveBeenCalledWith(
      expect.stringContaining("private/local addresses")
    );
    expect(registerTool).not.toHaveBeenCalled();
  });

  it("rejects bracketed fe80 link-local IPv6", () => {
    const logger = { debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() };
    const registerTool = vi.fn();
    plugin.register(
      createTestPluginApi({
        pluginConfig: { apiUrl: "http://[fe80::1]/api" },
        logger,
        registerTool,
      })
    );
    expect(logger.error).toHaveBeenCalledWith(
      expect.stringContaining("private/local addresses")
    );
  });

  it("rejects non-http protocols", () => {
    const logger = { debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() };
    const registerTool = vi.fn();
    plugin.register(
      createTestPluginApi({
        pluginConfig: { apiUrl: "ftp://evil.com/exploit" },
        logger,
        registerTool,
      })
    );
    expect(logger.error).toHaveBeenCalledWith(
      expect.stringContaining("must use http or https")
    );
  });

  it("allows valid public https URL", () => {
    const registerTool = vi.fn();
    plugin.register(
      createTestPluginApi({
        pluginConfig: { apiUrl: "https://custom-api.example.com" },
        registerTool,
      })
    );
    expect(registerTool).toHaveBeenCalledTimes(2);
  });

  it("defaults to production URL when apiUrl is omitted", () => {
    const registerTool = vi.fn();
    plugin.register(
      createTestPluginApi({
        pluginConfig: {},
        registerTool,
      })
    );
    expect(registerTool).toHaveBeenCalledTimes(2);
  });

  it("gracefully handles invalid config without crashing host", () => {
    const logger = { debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() };
    const registerTool = vi.fn();
    plugin.register(
      createTestPluginApi({
        pluginConfig: { apiUrl: "not-a-url" },
        logger,
        registerTool,
      })
    );
    // Should log error instead of throwing
    expect(logger.error).toHaveBeenCalled();
    expect(registerTool).not.toHaveBeenCalled();
  });
});

// ============================================================================
// Critical #1 & #5: Tool execution tests with consistent error handling
// ============================================================================

describe("search_actions tool execution", () => {
  beforeEach(() => {
    mockSearchActions.mockReset();
    mockGetActionByAreaId.mockReset();
  });

  it("returns search results on success", async () => {
    const markdown = "## Results\n\nFound 2 actions.";
    mockSearchActions.mockResolvedValue(markdown);

    const { tools } = registerAndCapture();
    const tool = tools.get("search_actions")!;
    const result = await tool.execute("call-1", {
      query: "airbnb search",
      domain: "airbnb.com",
      page: 1,
      page_size: 10,
    }) as { content: { text: string }[]; details: Record<string, unknown> };

    expect(result.content[0].text).toBe(markdown);
    expect(result.details).toEqual({ query: "airbnb search", domain: "airbnb.com" });
    expect(mockSearchActions).toHaveBeenCalledWith(
      expect.objectContaining({
        query: "airbnb search",
        domain: "airbnb.com",
        page: 1,
        page_size: 10,
      })
    );
  });

  it("handles ActionbookError with code", async () => {
    mockSearchActions.mockRejectedValue(
      new ActionbookError("RATE_LIMITED", "Too many requests")
    );

    const { tools } = registerAndCapture();
    const tool = tools.get("search_actions")!;
    const result = await tool.execute("call-2", { query: "test" }) as {
      content: { text: string }[];
      details: Record<string, unknown>;
    };

    expect(result.content[0].text).toContain("Failed to search actions: Too many requests");
    expect(result.details).toEqual({
      error: "Too many requests",
      code: "RATE_LIMITED",
    });
  });

  it("handles ActionbookError with markdown message passthrough", async () => {
    mockSearchActions.mockRejectedValue(
      new ActionbookError("NOT_FOUND", "## Error\n\nNo actions found for query.")
    );

    const { tools } = registerAndCapture();
    const tool = tools.get("search_actions")!;
    const result = await tool.execute("call-3", { query: "nonexistent" }) as {
      content: { text: string }[];
      details: Record<string, unknown>;
    };

    // Markdown errors from SDK should pass through directly
    expect(result.content[0].text).toBe("## Error\n\nNo actions found for query.");
    expect(result.details.code).toBe("NOT_FOUND");
  });

  it("handles generic Error", async () => {
    mockSearchActions.mockRejectedValue(new Error("Network timeout"));

    const { tools } = registerAndCapture();
    const tool = tools.get("search_actions")!;
    const result = await tool.execute("call-4", { query: "test" }) as {
      content: { text: string }[];
      details: Record<string, unknown>;
    };

    expect(result.content[0].text).toContain("Failed to search actions: Network timeout");
    expect(result.details).toEqual({ error: "Network timeout" });
  });

  it("handles non-Error throw", async () => {
    mockSearchActions.mockRejectedValue("string error");

    const { tools } = registerAndCapture();
    const tool = tools.get("search_actions")!;
    const result = await tool.execute("call-5", { query: "test" }) as {
      content: { text: string }[];
      details: Record<string, unknown>;
    };

    expect(result.content[0].text).toContain("Unknown error");
  });
});

describe("get_action_by_area_id tool execution", () => {
  beforeEach(() => {
    mockSearchActions.mockReset();
    mockGetActionByAreaId.mockReset();
  });

  it("returns action details on success", async () => {
    const markdown = "## Action: airbnb.com:/:default\n\nFull details here.";
    mockGetActionByAreaId.mockResolvedValue(markdown);

    const { tools } = registerAndCapture();
    const tool = tools.get("get_action_by_area_id")!;
    const result = await tool.execute("call-1", {
      area_id: "airbnb.com:/:default",
    }) as { content: { text: string }[]; details: Record<string, unknown> };

    expect(result.content[0].text).toBe(markdown);
    expect(result.details).toEqual({ area_id: "airbnb.com:/:default" });
  });

  it("handles ActionbookError with markdown passthrough", async () => {
    mockGetActionByAreaId.mockRejectedValue(
      new ActionbookError("NOT_FOUND", "## Error\n\nAction not found.")
    );

    const { tools } = registerAndCapture();
    const tool = tools.get("get_action_by_area_id")!;
    const result = await tool.execute("call-2", {
      area_id: "unknown:/:id",
    }) as { content: { text: string }[]; details: Record<string, unknown> };

    expect(result.content[0].text).toBe("## Error\n\nAction not found.");
    expect(result.details.code).toBe("NOT_FOUND");
  });

  it("handles ActionbookError without markdown prefix", async () => {
    mockGetActionByAreaId.mockRejectedValue(
      new ActionbookError("API_ERROR", "Internal server error")
    );

    const { tools } = registerAndCapture();
    const tool = tools.get("get_action_by_area_id")!;
    const result = await tool.execute("call-3", {
      area_id: "airbnb.com:/:default",
    }) as { content: { text: string }[]; details: Record<string, unknown> };

    expect(result.content[0].text).toBe(
      "## Error\n\nFailed to get action: Internal server error"
    );
    expect(result.details.code).toBe("API_ERROR");
  });

  it("handles generic Error consistently with search_actions", async () => {
    mockGetActionByAreaId.mockRejectedValue(new Error("Connection refused"));

    const { tools } = registerAndCapture();
    const tool = tools.get("get_action_by_area_id")!;
    const result = await tool.execute("call-4", {
      area_id: "airbnb.com:/:default",
    }) as { content: { text: string }[]; details: Record<string, unknown> };

    expect(result.content[0].text).toContain(
      "Failed to get action: Connection refused"
    );
    expect(result.details).toEqual({ error: "Connection refused" });
  });

  it("handles non-Error throw", async () => {
    mockGetActionByAreaId.mockRejectedValue(42);

    const { tools } = registerAndCapture();
    const tool = tools.get("get_action_by_area_id")!;
    const result = await tool.execute("call-5", {
      area_id: "airbnb.com:/:default",
    }) as { content: { text: string }[]; details: Record<string, unknown> };

    expect(result.content[0].text).toContain("Unknown error");
  });
});
