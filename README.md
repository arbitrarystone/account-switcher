# Account Switcher

> 热切换 Claude Code / Codex 多账号配置的桌面应用 —— 用不同账号的额度并发做不同任务。

内嵌多标签终端，每个标签 = 一个绑定了特定账号的隔离会话。开标签 1 用账号 A 跑 Claude、标签 2 用账号 B 跑 Codex，两个任务在 app 内真并发、互不串号。

## 功能

- **多账号管理** —— 管理多个 Claude Code / Codex 中转账号（名称 / BASE_URL / Token / 可选默认模型）。Token 只进系统钥匙串（macOS Keychain / Windows Credential Manager），绝不落明文。支持克隆账号到另一工具。
- **起任务** —— 选项目目录 + 账号，点「起任务」开一个内嵌终端会话，自动注入对应账号的隔离环境运行 `claude` / `codex`。
- **并发隔离** —— 多账号会话以标签页同屏并发，env 真隔离：标签 A 用账号 A、标签 B 用账号 B，token 互不串号（有集成测试正面验证）。
- **全局快切** —— 把账号设为某工具的全局默认，写入 `~/.claude/settings.json` / `~/.codex/config.toml`，app 外的终端也跟随。
- **用量与记忆** —— 本地 SQLite 统计每账号会话次数 / 时长 / 最近使用；记住每个项目上次用的账号，下次选目录自动预填。

## 技术栈

Tauri v2 · Rust · React 19 · TypeScript · Vite · xterm.js · portable-pty · rusqlite · keyring

## 适配机制

| 工具 | 按会话隔离（起任务） | 全局默认 |
|---|---|---|
| **Claude Code** | 注入 `ANTHROPIC_BASE_URL` + `ANTHROPIC_AUTH_TOKEN` | 写 `settings.json` 的 `env` 块 |
| **Codex** | `-c` 内联新建 provider + token env，不碰全局文件 | 写 `config.toml` 的 `model_provider`（token 走 `env_key`）|

## 开发

前置：Node.js 18+、pnpm、Rust（rustup）。

```bash
pnpm install
pnpm tauri dev      # 开发（前后端热重载）
```

## 构建

```bash
pnpm tauri build    # macOS 出 .app / .dmg
```

产物在 `src-tauri/target/release/bundle/`。

## 测试

```bash
cd src-tauri && cargo test     # 40 个单元/集成测试
cargo clippy --all-targets -- -D warnings
```

## 安全

- Token 仅存系统钥匙串，不写入明文配置或日志。
- 起任务时从**干净 env 基底**构造子进程环境，剔除可能劫持鉴权的脏变量（`ANTHROPIC_API_KEY` / `CLAUDE_CODE_USE_BEDROCK` / `AWS_*` 等）。
- 改写全局配置采用**原子写**（临时文件 + rename）+ 写前**备份**。
- ⚠️ 全局默认会把 Claude 的 Token 明文写入 `settings.json`（设置时有确认弹窗）；Codex 全局默认 token 走 `env_key` 环境变量，app 外使用需自行设置。

## 设计文档

完整设计见 [docs/specs/account-switcher-v1.md](docs/specs/account-switcher-v1.md)。
