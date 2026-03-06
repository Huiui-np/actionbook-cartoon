# Actionbook 架构理解与增强方案

> 更新时间：2024-03-04
>
> 本文档记录对 Actionbook 现有架构的正确理解，以及参考 Anything API 的增强方案。

## TL;DR

**现状**: Actionbook 有预置的 Playbook（网站使用说明书），但 Agent 需要手动解析 + 组合命令。

**目标**: 从 Playbook → 生成可执行的函数/MCP Tool，Agent 可以一键调用。

**方案**: Playbook → ExecutableScenario → Generated Code → MCP Tool

---

## 目录

- [现有架构理解](#现有架构理解)
- [核心概念澄清](#核心概念澄清)
- [当前痛点](#当前痛点)
- [增强方案](#增强方案)
- [实施路线图](#实施路线图)

---

## 现有架构理解

### 完整流程

```
Playbook Builder (LLM 自动探索)
    ↓
生成 7-section Playbook 文档
    ↓
存储到数据库（带 embeddings）
    ↓
MCP Server 提供查询接口
    ↓
Agent 手动解析 + 组合命令
```

### Playbook 的 7-section 结构

```markdown
## Section 0: Page URL
- URL 参数说明
- 动态参数识别

## Section 1: Page Overview
- 页面核心业务目标
- 主要功能描述

## Section 2: Function Summary
- 功能列表（自然语言）

## Section 3: Page Structure Summary ⭐ (关键)
- 布局模块
- CSS selectors（Agent 需要的！）

## Section 4: DOM Structure Instance
- 模式识别
- HTML 代码片段

## Section 5: Parsing & Processing Summary
- 数据提取场景

## Section 6: Operation Summary
- 交互操作说明
```

### 现有数据流

```typescript
// 1. Playbook Builder 生成
{
  title: "Airbnb Search Page",
  content: `
## Section 3: Page Structure
- Location input: #bigsearch-query-location-input
- Search button: [data-testid="search-button"]
...
  `
}

// 2. Agent 查询
const result = await mcpClient.call('search_actions', {
  query: 'Airbnb search'
});

// 3. Agent 手动解析 + 组合
const selector = extractSelector(result.content);
await actionbook.browser.type(selector, 'Paris');
await actionbook.browser.click('[data-testid="search-button"]');
```

---

## 核心概念澄清

### ✅ Playbook = 网站使用说明书

- **性质**: 描述性文档（markdown）
- **内容**: 页面功能 + CSS selectors + 操作说明
- **用途**: 供 Agent 阅读和理解
- **生成**: 自动（LLM + browser automation）

### ✅ UserScenario = 用户流程描述

```typescript
{
  name: "Search for accommodation",
  steps: [
    "Enter location",      // 自然语言
    "Select dates",        // 自然语言
    "Click search"         // 自然语言
  ]
}
```

- **特点**: 描述 WHAT（做什么），不是 HOW（怎么做）
- **问题**: Agent 需要手动将 steps → actionbook 命令

### ❌ 缺少：可执行的 Workflow

- **当前**: Playbook（文档）+ UserScenario（描述）
- **缺少**: 可直接调用的函数/API

---

## 当前痛点

### 痛点 1: 手动解析 Playbook

**现状：**
```typescript
// Agent 拿到的是 markdown 文档
const playbook = `## Section 3: Page Structure\n- Location input: #location`;

// Agent 需要解析 markdown、提取选择器、匹配操作类型
```

### 痛点 2: 每次手动组合命令

**现状：**
```typescript
// Agent 每次搜索都要写这些：
await actionbook.browser.type('#location', location);
await actionbook.browser.type('[data-testid="checkin"]', checkIn);
await actionbook.browser.click('[data-testid="search-button"]');
```

### 痛点 3: UserScenario 不可执行

**现状：**
```typescript
{
  steps: ["Enter location", "Select dates", "Click search"]
  // 自然语言，不是结构化操作
}
```

---

## 增强方案

### 核心思路

```
Playbook（说明书）
    ↓
ExecutableScenario（可执行工作流）
    ↓
Generated Function（生成的 TypeScript 函数）
    ↓
MCP Tool（Agent 可直接调用）
```

### Phase 1: Playbook → Executable Scenario

#### 数据模型

```typescript
/**
 * 可执行的步骤（结构化，不是 string）
 */
interface ExecutableStep {
  action: 'click' | 'type' | 'wait' | 'extract';
  selector: string;              // 从 Playbook Section 3 提取
  value?: string | { param: string }; // 支持参数化
  description: string;           // 人类可读
}

/**
 * 可执行的场景
 */
interface ExecutableScenario {
  id: string;
  name: string;
  parameters: Parameter[];       // 函数参数
  steps: ExecutableStep[];       // 结构化步骤
  code: string;                  // 生成的 TypeScript 代码
  playbookId: number;
}
```

#### 实现流程

```typescript
// 1. 解析 Playbook Section 3
const selectorMap = parseSelectorsMost(playbookSection3);
// → { 'Location input': '#location', 'Search button': '[data-testid="search-button"]' }

// 2. LLM 将 UserScenario.steps（自然语言）→ ExecutableStep（结构化）
const executableSteps = await llm.convertToExecutableSteps({
  scenario: ["Enter location", "Click search"],
  availableSelectors: selectorMap
});
// → [
//     { action: "type", selector: "#location", value: { param: "location" } },
//     { action: "click", selector: "[data-testid='search-button']" }
//   ]

// 3. 生成 TypeScript 代码
const code = generateTypeScriptCode(executableSteps);
```

#### 生成示例

**输入：UserScenario**
```typescript
{
  name: "Search for accommodation",
  steps: ["Enter location", "Select dates", "Click search"]
}
```

**输出：ExecutableScenario**
```typescript
{
  id: "exec-123",
  name: "Search for accommodation",
  parameters: [
    { name: "location", type: "string", required: true },
    { name: "checkIn", type: "string", required: true },
    { name: "checkOut", type: "string", required: true }
  ],
  steps: [
    {
      action: "type",
      selector: "#bigsearch-query-location-input",
      value: { param: "location" },
      description: "Enter location"
    },
    {
      action: "type",
      selector: "[data-testid='checkin']",
      value: { param: "checkIn" },
      description: "Select check-in date"
    },
    {
      action: "click",
      selector: "[data-testid='search-button']",
      description: "Click search"
    }
  ],
  code: `
export async function searchForAccommodation(
  location: string,
  checkIn: string,
  checkOut: string
): Promise<void> {
  const browser = await actionbook.getBrowser();
  await browser.type('#bigsearch-query-location-input', location);
  await browser.type('[data-testid="checkin"]', checkIn);
  await browser.click('[data-testid="search-button"]');
}
  `
}
```

### Phase 2: MCP Tool Registration

```typescript
// 动态注册生成的函数为 MCP tool
{
  name: 'search_for_accommodation',
  description: 'Search for accommodation on Airbnb',
  inputSchema: {
    type: 'object',
    properties: {
      location: { type: 'string' },
      checkIn: { type: 'string' },
      checkOut: { type: 'string' }
    },
    required: ['location', 'checkIn', 'checkOut']
  }
}

// Agent 调用
await mcpClient.call('search_for_accommodation', {
  location: 'Paris',
  checkIn: '2024-03-10',
  checkOut: '2024-03-15'
});
```

### Phase 3: Workflow Recording + Enhancement

```bash
# 录制执行轨迹
actionbook workflow record --task "搜索住宿"

# 执行成功 → 优化 Playbook Section 3（新增/更新选择器）
# 重新生成 ExecutableScenarios
```

---

## 实施路线图

### 里程碑 1: MVP（2-3 周）

**目标**: 从 Playbook 生成 ExecutableScenarios

**交付物**:
1. ✅ `PlaybookSelectorParser` - 解析 Section 3
2. ✅ `ScenarioCodegen` - UserScenario → ExecutableScenario
3. ✅ 数据库扩展（`executable_scenarios` 表）
4. ✅ 集成到 `PlaybookBuilder.build()`

**验证**: 生成 20+ ExecutableScenarios，成功率 >= 80%

### 里程碑 2: MCP Tool（1-2 周）

**目标**: 注册为 MCP tools

**交付物**:
1. ✅ `loadExecutableScenarioTools()` - 动态加载
2. ✅ `executeScenario()` - 执行函数
3. ✅ 集成到 MCP Server

**验证**: Agent 可调用生成的 tools，成功率 >= 80%

### 里程碑 3: Recording + Enhancement（2-3 周）

**目标**: 从执行轨迹优化 Playbook

**交付物**:
1. ✅ actionbook-rs `workflow record` 命令
2. ✅ `PlaybookEnhancer` - 优化 Playbook
3. ✅ 自动重新生成 ExecutableScenarios

**验证**: 录制 10 个轨迹，优化 5+ Playbooks

---

## 对比：现有 vs 增强版

### 现有流程（需要 Agent 手动组合）

```typescript
// 1. 查询 Playbook
const playbook = await mcpClient.call('get_action_by_id', {
  id: 'airbnb.com/search'
});

// 2. 手动解析
const selectors = parsePlaybook(playbook.content);

// 3. 手动组合命令
await actionbook.browser.type(selectors.location, 'Paris');
await actionbook.browser.click(selectors.searchButton);
```

### 增强后（一行调用）

```typescript
await mcpClient.call('search_for_accommodation', {
  location: 'Paris',
  checkIn: '2024-03-10',
  checkOut: '2024-03-15'
});
```

---

## 与 Anything API 对比

| 维度 | Anything API | Actionbook（增强后） |
|------|--------------|---------------------|
| **知识库** | ❌ 无 | ✅ Playbook（预置） |
| **探索** | ✅ 实时探索 | ✅ 预置 + 轨迹学习 |
| **可执行性** | ✅ 生成函数 | ✅ ExecutableScenario + 代码生成 |
| **复用** | ✅ 函数调用 | ✅ MCP Tool 调用 |
| **验证** | ❓ 未知 | ✅ 社区验证 + 成功率跟踪 |
| **开源** | ❌ 闭源 | ✅ 开源 |

**Actionbook 的独特优势**:
1. Playbook 作为预置知识库（更快）
2. 结构化的 7-section 格式（更可靠）
3. 开源 + 可自托管
4. 自学习闭环（轨迹 → 优化 Playbook）

---

## 总结

**关键洞察**: Actionbook 已经有了很好的基础（Playbook = 网站说明书），现在需要的是：**从说明书 → 可执行代码**。

**实现路径**: Playbook → ExecutableScenario → Generated Function → MCP Tool

**预期效果**:
- Agent: 从"手动解析 + 组合"→"一行调用"
- Actionbook: 从"查询式文档库"→"可执行 API 平台"
- 用户: 从"需要理解 Playbook"→"自然语言任务执行"
