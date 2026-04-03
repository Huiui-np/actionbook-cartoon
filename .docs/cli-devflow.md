# CLI Feature Development Workflow

Standard CLI feature development follows a 6-phase TDD + dual review workflow.




## Phase 0: 分析对齐

- 从 main 获取最新代码
- 切分支。
- Read PRD/SPEC, compare reference implementations
- Confirm plan with user
- Create feature branch (`feature/xxx`)


## Phase 1: 写测试 (TDD)

- E2E + UT contract tests first
- Assertions must verify values (not just types), use strict whitelists, and include negative baselines
- Contract tests should fail against current code
- Tests use eval-injected fixtures following existing patterns in `tests/e2e/interaction.rs`
- Use playground (`cli-e2e-playground`) for manual verification scenarios

## Phase 2: Review

- Parallel code-reviewer agent + codex challenge on tests
  - claude code
  - codex: use codex cli 
- Fix all findings
- Check fixture validity, edge cases, assertion strength

## Phase 3: 实现

- Implement to pass all tests
- Must pass:
  ```bash
  cargo fmt -p actionbook-cli
  cargo clippy -p actionbook-cli --all-targets -- -D warnings
  cargo test -p actionbook-cli --lib
  ```

## Phase 4: 双 Review 实现

**本地 review（必须在 push 前完成）：**
- Parallel code-reviewer agent + codex challenge on tests
  - claude code: use /code-review
  - codex: use codex cli 
- Fix all findings before push

**远程 review（push 后）：**
- Codex bot（chatgpt-codex-connector）自动 review PR
- 处理方式见 Phase 6

## Phase 5: 真实浏览器验证

- Build release: `cargo build --release`
- Kill old daemon to ensure latest binary: `pkill -f actionbook`（或 daemon 自动重启已支持版本不匹配自动重启）
- Test on real pages or playground (localhost:5173)
- Compare with agent-browser output
- Every bug found MUST have a test added

## Phase 6: PR → CI → Codex Bot Review Loop

1. Push branch, create PR targeting `main`
2. Run `/review` 本地 review（如 Phase 4 未做）
3. Codex bot review loop:
   - 每 30 秒检查 PR review comments
   - 发现新 comment 时：
     - **will fix**: 修复代码，commit + push，reply 到 comment 说明修复内容和 commit hash
     - **won't fix**: reply 到 comment 说明原因（如 "flock provides serialization"）
   - 修复后 `@chatgpt-codex-connector please review again` 触发下一轮
   - 重复直到 10 分钟无新 comment 或连续 2 轮无 P1
4. CI all green then merge to `main`

**Reply 格式：**
```
# fix:
Fixed in {commit_hash}. {一句话说明改了什么}

# won't fix:
Acknowledged. {原因}. {为什么现有机制已经 cover 了这个 case}
```

## Key Principles

- **UT alone misses runtime integration bugs** — always do real browser verification
- **三路 review** — Claude structured + Codex adversarial + Codex bot（三个视角）
- **Real browser testing is irreplaceable** — headed mode for visual confirmation
- **Every runtime bug must be backfilled** with test coverage
- **Daemon 版本自动重启** — rebuild 后 CLI 自动检测版本不匹配并重启 daemon
