# Account Switcher v1 — 设计规格

> 状态：**已定稿（brainstorm 全程确认）** · 日期：2026-06-30 · 来源会话：`95a7d753-5530-4304-9bc0-9a5c2a62a480`

---

## 1. 产品概述

**一句话**：一款跨平台桌面应用，用于热切换 Claude Code / Codex 的多账号（第三方中转）配置，并以**内嵌多标签终端**的形态，让"用不同账号的额度并发做不同任务"成为最自然的操作。

**核心痛点**：用户持有多个中转账号（各有独立额度），希望在不同项目/任务中用不同账号，且能**同时并发**跑而互不串号。

**核心模型**：**每个终端标签页 = 一个绑定了特定账号的隔离会话**。开标签 1 用账号 A 跑 Claude、开标签 2 用账号 B 跑 Codex，两个任务在 app 内真并发，用量分别精确统计。

### 目标用户
- 持有多个 Claude/Codex 第三方中转账号的开发者。

### v1 范围（YAGNI）
- ✅ 账号 CRUD（中转账号：名称 / BASE_URL / Token / 可选默认模型）
- ✅ 内嵌多标签终端，按会话注入账号 env，真并发隔离
- ✅ 全局默认账号快切（改写工具的全局配置文件）
- ✅ 记忆规则（项目上次账号、工具默认账号）自动预填
- ✅ 本地用量统计（次数 / 时长 / 项目维度）
- ✅ Windows + macOS 双平台

### 非目标（v1 不做）
- ❌ 查询远端账号余额（只做本地用量统计）
- ❌ 官方登录账号 / API Key 账号类型（v1 只做中转账号）
- ❌ 团队协作 / 云同步
- ❌ 内嵌终端之外的复杂 IDE 功能

---

## 2. 需求小结（已确认）

| 维度 | 决定 |
|---|---|
| 平台 | Windows + macOS 双平台 |
| 工具 | Claude Code + Codex，预留可插拔适配层 |
| 账号类型 | 全部第三方中转/代理：`(名称, BASE_URL, Token, 可选模型)` |
| 切换模型 | 全局默认快切 **+** 按任务/会话并发隔离 |
| 形态 | v1 完整桌面 GUI，内嵌多标签终端 |
| 主窗口布局 | **A · 终端工作台**：终端为主舞台 + 左侧账号栏 + 顶部起任务条 |
| 额度 | 只做本地用量统计（次数/时长/项目），不查远端余额 |
| 挑选记忆 | 记忆每个项目上次账号 + 每个工具默认账号，自动带出可改 |

---

## 3. 技术栈

| 层 | 选型 | 说明 |
|---|---|---|
| 外壳 | **Tauri v2** | 安装包小（~10-15MB）、内存低、跨平台托盘 |
| 后端 | **Rust** | 适配器、PTY、配置写入、用量、钥匙串 |
| 前端 | **React + TypeScript + Vite** | UI |
| 终端渲染 | **xterm.js** | 多标签终端 |
| PTY | **portable-pty**（Rust） | 跨平台伪终端 |
| 用量存储 | **SQLite**（`rusqlite` 或 `sqlx`） | 便于聚合统计 |
| 密钥存储 | **系统钥匙串**（`keyring` crate） | macOS Keychain / Windows Credential Manager |
| 偏好存储 | **JSON 文件**（`prefs.json`） | 记忆规则、UI 状态 |

### 可复用前作（终端层不手搓）
- [`Tnze/tauri-plugin-pty`](https://github.com/Tnze/tauri-plugin-pty) — 一行 Rust 接入 PTY
- [`Shabari-K-S/terminon`](https://github.com/Shabari-K-S/terminon) — 多标签生产级终端
- [`emee-dev/terax-ai-tauri-terminal`](https://github.com/emee-dev/terax-ai-tauri-terminal) — Tauri2 + React19 + TS + xterm + portable-pty，<10MB

---

## 4. 整体架构

**三层 + 可插拔适配层**：

```
┌──────────────────────────────────────────────────────────┐
│  前端层 (React + xterm.js)                                  │
│  · 账号侧栏  · 起任务条  · 多标签终端区  · 用量看板          │
└───────────────┬──────────────────────────────────────────┘
                │ Tauri invoke / event
┌───────────────▼──────────────────────────────────────────┐
│  Tauri 命令层 (commands.rs)                                 │
│  account_*  ·  launch_session  ·  set_default  ·  usage_*  │
└───────────────┬──────────────────────────────────────────┘
                │
┌───────────────▼──────────────────────────────────────────┐
│  Rust 核心层                                                │
│  ┌────────────┐ ┌──────────┐ ┌────────────┐ ┌──────────┐ │
│  │ 适配层      │ │ PTY 管理  │ │ 配置写入器  │ │ 用量记录 │ │
│  │ (trait)    │ │          │ │ (原子+备份) │ │ (SQLite) │ │
│  │ Claude     │ │          │ │            │ │          │ │
│  │ Codex      │ │          │ │            │ │          │ │
│  └────────────┘ └──────────┘ └────────────┘ └──────────┘ │
│  ┌────────────┐ ┌──────────┐                              │
│  │ 钥匙串      │ │ 记忆规则  │                              │
│  │ (keyring)  │ │ (prefs)  │                              │
│  └────────────┘ └──────────┘                              │
└───────────────────────────────────────────────────────────┘
```

### 适配层 trait（可插拔）

```rust
/// 每个工具（claude / codex）实现一个适配器
trait ToolAdapter {
    /// 工具标识
    fn tool(&self) -> Tool;

    /// 按会话隔离：基于干净 env 基底，构造启动子进程所需的 (program, args, env)
    /// 完全不碰全局配置文件
    fn build_session_launch(&self, account: &Account, token: &str, project_dir: &Path)
        -> LaunchSpec;

    /// 全局默认：把该账号写入工具的全局配置文件（原子写 + 备份）
    fn write_global_default(&self, account: &Account, token: &str) -> Result<()>;

    /// 清除/重置全局默认（可选）
    fn clear_global_default(&self) -> Result<()>;
}

struct LaunchSpec {
    program: String,        // "claude" / "codex"
    args: Vec<String>,
    env: BTreeMap<String, String>,  // 仅本账号变量（已脱脏）
    cwd: PathBuf,
}
```

---

## 5. 核心数据流

### 🟦 流程一：起任务（并发隔离）— 主路径

1. 用户在起任务条选好 **项目目录 + 工具 + 账号**（记忆规则已预填，可改）→ 点"起任务"
2. 前端 `invoke('launch_session', { tool, accountId, projectDir })`
3. Rust：
   a. 从**钥匙串**取该账号 Token
   b. 调对应适配器 `build_session_launch()` → 基于**干净 env 基底**构造隔离 env（见 §7 env 卫生）
   c. **portable-pty** 在 `projectDir` 拉起 `claude` / `codex` 子进程
4. PTY 输出经 Tauri **event 流**推给前端 → xterm 渲染成**新标签页**
5. **用量记录器**：记 `startedAt`（status=running）；会话退出记 `endedAt/durationSec/exitCode`
6. **记忆规则**更新："该项目该工具的上次账号 = 当前账号"
7. ⚠️ **全程不碰任何全局配置文件** → 这是标签1用A、标签2用B 能真并发的根本保证

### 🟥 流程二：全局快切（设默认）— 辅路径

1. 用户把账号 X 设为某工具的默认★
2. 前端 `invoke('set_default', { tool, accountId })`
3. Rust 调适配器 `write_global_default()`：
   - Claude → 写 `~/.claude/settings.json` 的 `env` 块
   - Codex → 写 `~/.codex/config.toml` 的 provider + `model_provider`
   - **原子写**（临时文件 + rename）+ **先备份原文件**
4. 之后**外部终端**（app 之外）起的会话也跟随账号 X

> 设计要点：只有「全局默认」才改写全局文件；「按会话」永不改写 → 两条路径互不干扰。

---

## 6. 适配器机制（已查实验证）

### ✅ Claude Code

**注入变量**：
- `ANTHROPIC_BASE_URL` — 重定向全部流量到中转
- `ANTHROPIC_AUTH_TOKEN` — 中转的 Bearer Token（**注意是 AUTH_TOKEN 不是 API_KEY**）
- 可选 `ANTHROPIC_MODEL` — 默认模型覆盖

**鉴权优先级阶梯**（注入时必须避免被更高优先级变量劫持）：
```
云厂商凭证 (Bedrock/Vertex) > ANTHROPIC_AUTH_TOKEN > ANTHROPIC_API_KEY > OAuth 登录
```

**按会话**：注入上述 env 启动子进程。
**全局默认**：写 `~/.claude/settings.json`：
```json
{
  "env": {
    "ANTHROPIC_BASE_URL": "https://relay.example.com",
    "ANTHROPIC_AUTH_TOKEN": "<token 或经 apiKeyHelper 间接取>"
  }
}
```

### ✅ Codex

**关键**：内置 `openai` provider 的 base_url **不能**在 TOML 里覆盖（ID 保留）。中转的正确做法是**新建 provider**。

**按会话**（不碰全局文件，天然隔离）：
```bash
codex \
  -c model_providers.relayX.name="relayX" \
  -c model_providers.relayX.base_url="https://relay.example.com/v1" \
  -c model_providers.relayX.env_key="RELAYX_TOKEN" \
  -c model_provider="relayX"
# 同时设 env: RELAYX_TOKEN=<token>
```

**全局默认**：写 `~/.codex/config.toml`：
```toml
model_provider = "relayX"

[model_providers.relayX]
name = "relayX"
base_url = "https://relay.example.com/v1"
env_key = "RELAYX_TOKEN"
```
（或用 `--profile` 叠加 `~/.codex/<name>.config.toml`）

**参考来源**：
- [Codex 配置参考](https://developers.openai.com/codex/config-reference)
- [Codex 高级配置](https://developers.openai.com/codex/config-advanced)
- [Claude Code 环境变量官方文档](https://code.claude.com/docs/en/env-vars)

---

## 7. 数据模型

### Account（元数据，**不含 Token**）
Token 单独存系统钥匙串，此处仅留引用 `tokenRef`：
```
id          : string (uuid)
name        : string
tool        : "claude" | "codex"     // 账号绑定单一工具
baseUrl     : string
model?      : string                 // 默认模型覆盖
tokenRef    : string                 // 钥匙串键名
tags?       : string[]
createdAt   : timestamp
updatedAt   : timestamp
```
> 同一中转若两边都支持 → 建两个账号；提供「克隆到另一工具」便捷操作。

### UsageRecord（每会话一条）→ **SQLite**
```
id          : string
accountId   : string
tool        : string
projectDir  : string
startedAt   : timestamp
endedAt?    : timestamp
durationSec : int
status      : "running" | "exited" | "error"
exitCode?   : int
```

### Prefs / 记忆规则 → `prefs.json`
```jsonc
{
  "defaultAccountByTool": { "claude": "<id>", "codex": "<id>" },   // 全局默认★
  "lastAccountByProject": { "<path>::<tool>": "<id>" },            // 项目上次账号
  "ui": { /* 布局/主题等 */ }
}
```

---

## 8. 安全与错误处理（重点）

### Token 存储
- **只进系统钥匙串**（macOS Keychain / Windows Credential Manager）
- **绝不**落明文配置、**绝不**写日志/用量记录（统一脱敏）

### env 卫生（承接调研里的优先级阶梯坑）
拉起子进程时**从干净基底构造 env**，显式清理继承来的脏鉴权变量，只注入本账号变量：
```
清理清单（示例）：
  ANTHROPIC_API_KEY
  ANTHROPIC_AUTH_TOKEN (旧值)
  ANTHROPIC_BASE_URL   (旧值)
  CLAUDE_CODE_USE_BEDROCK
  CLAUDE_CODE_USE_VERTEX
  AWS_* / GOOGLE_* (相关云凭证)
  ... (Codex 同理清理旧 provider env)
```
→ 保证真隔离、不被高优先级变量劫持。

### 全局默认的 Token 落盘问题
- 写 settings.json / config.toml 时**尽量用 helper 命令间接取钥匙串**（Claude 的 `apiKeyHelper`、Codex 的 command-backed token），避免明文。
- 个别工具实在绕不开明文 → **显式提示并需用户确认**。
- ⚠️ 标注：这是仅「全局默认」路径才有的待定实现细节；「按会话」路径无此问题。

### 配置写入原子化
- 临时文件 + `rename`
- 写入前**备份原配置**
- 权限失败 → 只报错，**不破坏**现有文件

### 边界校验
- `baseUrl`：https / 格式校验
- `token`：非空
- `projectDir`：目录存在

### 启动失败处理
- `claude` / `codex` 不在 PATH → 明确提示 + 安装指引
- 401 / 连接错误 → 本就在真实终端里显现
- PTY 崩溃 → 标记会话 `error` 并保留退出信息

---

## 9. 测试策略（目标覆盖率 80%，重在核心逻辑）

| 类型 | 范围 |
|---|---|
| **单元** | 适配器的 env/args 构造（纯函数）、env 脱脏器、记忆规则解析、用量聚合 |
| **集成** | 配置写入器对着**临时 HOME** 跑（断言 settings.json/config.toml 内容、原子写、备份） |
| **E2E** | 用**假 CLI 脚本**（回显自己的 env）顶替 `claude`/`codex`（PATH 覆盖），断言会话拿到正确 BASE_URL/Token、断言用量入库；Tauri UI 用 `tauri-driver` 跑关键流 |

---

## 10. v1 里程碑（GUI 完整版，分 6 步）

| # | 里程碑 | 交付物 |
|---|---|---|
| **M1** | 账号管理 | CRUD + 钥匙串存 Token + 侧栏列表 + 克隆到另一工具 |
| **M2** | 单会话起任务 | 适配层(Claude/Codex) + PTY + 单终端标签 |
| **M3** | 并发隔离 | 多标签并发 + 隔离验证（假 CLI 回显 env 断言） |
| **M4** | 全局快切 | 配置写入器（原子+备份）+ 默认★ |
| **M5** | 记忆与用量 | 记忆规则 + 用量入库 + 用量看板 |
| **M6** | 打磨与跨平台 | Windows 验证 + 打包分发 |

---

## 11. 待定细节（Open Questions）

1. **全局默认明文兜底**：哪些中转工具支持 helper 间接取 token？不支持时的明文确认 UX 如何呈现。（M4 处理）
2. **用量"时长"语义**：是 PTY 进程存活时长，还是实际交互时长？v1 先用进程存活时长。
3. **Codex provider 命名**：`env_key` 与 provider id 的生成规则（避免多账号冲突），建议用账号 id 派生。
4. **打包签名**：macOS 公证 / Windows 签名证书，M6 处理。

---

## 附录：项目结构（规划）

```
account-switcher/
├── docs/specs/account-switcher-v1.md   # 本文档
├── package.json
├── vite.config.ts
├── index.html
├── src/                                # 前端 React/TS
│   ├── main.tsx
│   ├── App.tsx
│   ├── components/
│   │   ├── account-sidebar/
│   │   ├── launch-bar/
│   │   ├── terminal-tabs/
│   │   └── usage-dashboard/
│   ├── hooks/
│   ├── lib/                            # invoke 封装、类型
│   └── styles/
└── src-tauri/                          # 后端 Rust
    ├── Cargo.toml
    ├── tauri.conf.json
    └── src/
        ├── main.rs
        ├── commands.rs                 # Tauri 命令入口
        ├── account/                    # Account 模型 + 持久化
        ├── adapter/                    # ToolAdapter trait + claude/codex
        ├── pty/                        # PTY 管理
        ├── config_writer/              # 全局配置原子写 + 备份
        ├── usage/                      # SQLite 用量
        ├── keychain/                   # keyring 封装
        ├── prefs/                      # 记忆规则
        └── env_hygiene.rs              # env 脱脏器
```
```
