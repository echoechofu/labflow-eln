# 架构概览

## 正式桌面运行时

```text
React UI
  → @tauri-apps/api invoke
  → Tauri Rust command layer（单连接 Mutex）
  → domain/repository modules
  → SQLite + user filesystem
```

React 通过 `src/repository.ts` 调用 Tauri command；Rust 在应用启动时解析用户数据路径、创建 `files/`、打开 SQLite、应用 schema 演进并确保内置 Protocol。新工作区不自动写入示例 Experiment、Task 或 Record；领域写入（Task、Protocol execution、lineage、导出）由 Rust transaction 执行，避免 UI 直接操纵 SQLite。

正式打包的 macOS `.app` 与 Windows NSIS/MSI 安装版都使用 Tauri `frontendDist` 中的静态资源，业务运行时不依赖 Express 或 localhost HTTP API。

## 本地 Agent Interface

```text
Codex → LabFlow MCP（stdio）→ shared domain service → validation + transaction → SQLite
Desktop UI → Tauri command ────────────────┘
```

本地 `labflow-mcp` binary 与 Tauri command 共用 Rust Task service 和 canonical workspace 初始化。MCP 只声明结构化 tool schema、调用 service 并映射 domain error；它不接受数据库路径，也不向 Agent 暴露 SQLite schema。Desktop 窗口重新获得焦点时重载 store，以显示独立 MCP 进程已经提交的变化。

### 模块化 Agent 架构

`src-tauri/src/agent_interface/` 是适配器目录而非业务层。模块组成如下：

| 模块 | 职责 | 路由函数 |
| --- | --- | --- |
| `agent_interface::mod` | `LabFlowMcp` 服务外壳：共享连接、`AgentModuleError` 通用错误契约、`compose_module_routers` 组合点、`ServerHandler` 元信息 | — |
| `agent_interface::task_tools` | Task/Calendar 的 6 个 tools，全部调用 `task_service` | `LabFlowMcp::task_tools_router()` |
| `agent_interface::experiment_tools` | Experiment CRUD tools，调用 `experiment_service` | `LabFlowMcp::experiment_tools_router()` |
| `agent_interface::protocol_tools` | Protocol 模板读写 tools，使用 typed request schema 并调用拥有完整校验规则的 `protocol_service` | `LabFlowMcp::protocol_tools_router()` |
| `agent_interface::record_tools` | Record 列表/详情/正文编辑/删除 tools，调用 `record_service` | `LabFlowMcp::record_tools_router()` |

`#[tool_router(router = ..., vis = "pub(crate)")]` 宏允许每个 Agent 模块在单独的 `impl LabFlowMcp` 块中声明带名字的 `ToolRouter`，`compose_module_routers` 通过 `ToolRouter::new() + Self::<module>_router()` 把所有模块路由合并。新模块（Sample lineage、Terminal Assay、qPCR/ELISA/CCK8 Analysis）落地时，只需新增子模块并在合并点加一行，**不得在 MCP 层复制 Service 业务规则**。

Rust 服务层（`task_service`、`experiment_service`、`protocol_service`、`record_service`）同时被 Tauri command 与 MCP tool 调用，保证 Desktop UI 与 Agent 看到同一份 validation 与 transaction。Tauri command 现在是薄壳：`save_experiment`/`delete_experiment`/`save_user_protocol`/`save_protocol_template_version`/`update_record_body`/`delete_record`/`start_task_record` 全部转发到对应 service。

当前 tools:

- **Calendar / Task**：`labflow_list_experiments`、`labflow_list_tasks`、`labflow_get_task`、`labflow_create_task`、`labflow_update_task`、`labflow_delete_task`
- **Experiment**：`labflow_get_experiment`、`labflow_save_experiment`、`labflow_delete_experiment`
- **Protocol**：`labflow_list_protocols`、`labflow_get_protocol`、`labflow_create_protocol`、`labflow_save_protocol_version`、`labflow_delete_protocol`
- **Record**：`labflow_list_records`、`labflow_get_record`、`labflow_update_record_body`、`labflow_delete_record`

Sample lineage、Terminal Assay、qPCR / ELISA / CCK8 Analysis 还没暴露。

### Agent 契约

对 Agent 客户端（Codex、ChatGPT、workbuddy 等）完整的调用约定见：

- `.agents/skills/labflow-agent/SKILL.md` — 架构契约与禁止项；
- `.agents/skills/labflow-calendar/SKILL.md` — 当前 Task 模块的具体工作流。

核心原则：Agent 不直接访问 SQLite；MCP 只声明 schema 并调用 service，映射 domain error；Desktop UI 与 Agent 复用同一 `task_service` 的 validation 与 transaction；新增 Agent 模块通过新增子模块注册 router，不得复制业务规则。

本机开发构建与 Codex 注册：

```bash
npm run mcp:build
codex mcp add labflow -- <repository>/eln-app/src-tauri/target/release/labflow-mcp
codex mcp get labflow
```

注册的是本地可执行文件，不配置或传递数据库路径。更新 binary 后需刷新 Codex task，令客户端重新发现 tools。

## Web 开发兼容层

非 Tauri 环境中，repository 会请求 `/api/store`；`server.ts` 提供 Express + `better-sqlite3` 的开发兼容层，并复用 Node `AppDataPathProvider`。该层用于网页开发、旧数据库迁移和基础测试兼容，不是正式桌面运行时的一部分。新 Sample/lineage 写入命令明确要求 Tauri desktop。

## 本地数据与路径隔离

平台 canonical 用户根目录：

```text
macOS:   ~/Library/Application Support/LabFlow/
Windows: %APPDATA%\LabFlow\

目录内部：
├── labflow.sqlite
├── files/
    └── exports/
├── backups/
└── import-staging/       （仅恢复期间）
```

Node 侧由 `AppDataPathProvider` 集中提供 `getAppDataDir()`、`getDatabasePath()`、`getAttachmentsDir()` 与相对附件定位器；Tauri 侧以平台 `data_dir()` 为 base，再追加稳定产品目录 `LabFlow`。UI、repository 和 domain 不应硬编码 OS 绝对路径。

旧的 `<project>/data/labflow.sqlite` 只在目标不存在时复制到 canonical 路径，复制后进行 SQLite integrity/table 验证；不会静默覆盖既有目标数据库。数据库、附件、导出文件若出现在 source/project directory，即为 P0 data-integrity failure。

SQLite 保存结构化领域数据及文件的**相对** locator（如 `files/exports/<id>/manifest.json`）；附件和 export manifest 文件实体存于用户根目录下的 `files/`。build/dist/target 的删除不应影响用户数据。

## 关键数据流

1. UI 载入 store：Tauri `get_store` 将 SQLite 行和 JSON 字段投影为 UI model。
2. Task 编辑：`save_task` 在 transaction 内校验并保存 Task/Experiment/Task relation。
3. Record 启动：`start_task_record` 调用 Protocol execution，在一个 transaction 内写入 Record、过程、Sample lineage、usage 和 Result。
4. 导出：创建 manifest 时从 SQLite 组装已保存的 Record snapshot、正文及关联数据，生成 SHA-256，并将 JSON 写到 `files/exports/`，随后在 SQLite 写 export 元数据。
5. 终末检测：Record Setup 在创建事务中保存 Assay Items；Plate Mapping 与 raw import 使用 focused Tauri commands 独立写入。raw 原文件进入 `files/<attachment-id>/`，解析值进入 SQLite，读取 workspace 时按 plate/well 生成 join dataset。qPCR 可继续建立 ΔCt/ΔΔCt 快照；保存时在同一 transaction 内插入分析行、向 Record 追加冻结文字并记录变更。ELISA/CCK-8 当前停在通用 join dataset。
6. 用户 Protocol：React 三步向导提交受限 Sample Flow 定义；focused Tauri command 在一个 transaction 中注册新 Sample 类型并写入 Protocol v1。模板编辑复制当前 schema 为新的 user version，只替换正文模板。用户删除自建 Protocol 时共享 service 原子删除主记录与版本，但保留 Sample Type 和历史 Record；Desktop 与 MCP 复用同一规则。Record 启动仍复用同一 transaction 执行器和 snapshot 机制。
7. 工作区备份：Tauri 用 SQLite Online Backup API 将正在使用的连接复制为一致性快照，再与 `files/` 和 checksum manifest 封装为 `.labflow-backup`。恢复先在 staging 只读校验，并创建当前工作区备份/本地 rollback snapshot；通过后替换式恢复并刷新 UI store。

旧的全量 `save_store` command 仍存在以兼容早期 store 形式，但一旦已有 `process_events` 就拒绝执行，以免覆盖 lineage；当前功能使用 focused commands。
