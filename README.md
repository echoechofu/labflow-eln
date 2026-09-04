# LabFlow — Local-first Electronic Lab Notebook (ELN)

<p align="center">
  <img src="eln-app/src/assets/hero.png" alt="LabFlow local-first electronic lab notebook logo" width="220" />
</p>

<p align="center">
  <strong>面向生物医学湿实验的本地优先电子实验记录本与实验管理软件</strong><br />
  A private, offline-first ELN for biomedical wet labs on macOS and Windows.
</p>

<p align="center">
  <a href="https://github.com/echoechofu/labflow-releases/releases/latest">下载最新版 / Download</a>
  ·
  <a href="docs/product/user-guide.md">用户手册</a>
  ·
  <a href="docs/architecture/overview.md">Architecture</a>
</p>

LabFlow 将实验日历、Protocol、实验 Record、Sample 输入输出关系、实验图片和 PDF 导出放进一个本地桌面应用。它适合需要安排细胞实验、复用实验方法、记录实际操作并追踪样本来源的研究人员、研究生和小型科研团队。

LabFlow is a local-first laboratory experiment management system and electronic lab notebook for biomedical research. It combines experiment planning, reusable protocols, structured records, sample tracking, image attachments and PDF export without requiring a cloud account.

## Why LabFlow?

- **实验计划 / Experiment planning**：用周日历安排和调整实验 Task，表达实验之间的前后依赖。
- **Protocol management**：保存内置或自建实验方法；每次执行都冻结版本快照，历史 Record 不会被后续模板修改。
- **Electronic lab records**：记录本次实验的真实操作、偏差和观察结果，并在正文中插入 WB、显微镜或其他实验图片。
- **Sample tracking and lineage**：保存输入、输出、消耗、派生、条件分组和孔板位置，支持追踪 Sample 来源。
- **Local data ownership**：SQLite 数据库、图片原件和预览均保存在用户本机，可导出完整工作区备份。
- **macOS and Windows**：提供 Apple Silicon macOS 与 Windows 10/11 x64 桌面安装包。
- **Agent-ready**：Desktop UI 与 MCP Agent Interface 复用同一套 Rust domain/service、validation 和 transaction。

## Core workflows

| Workflow | What LabFlow supports |
| --- | --- |
| Experiment calendar | 创建、编辑、关联和完成实验 Task；按周查看实验安排 |
| Protocol builder | 定义 Sample Flow、相同条件的多个输出，或按条件/剂量/时间分组输出 |
| Record | 从 Protocol 创建带版本快照的实验记录，编辑正文并插入实验图片 |
| Sample flow | CELL、PLATE、DISH、WELL、RNA、cDNA、PROTEIN、SUP 及自定义 Sample 类型 |
| Plate conditions | 条件分配可选映射孔板位置；Sample 类型与孔位互不绑定 |
| Terminal assays | qPCR、ELISA、CCK-8 的 Plate Mapping 与 Raw Data 保存骨架 |
| Export and backup | 系统打印、低内存 PDF，以及包含 SQLite 与附件的完整工作区备份 |

## Built-in biomedical protocols

当前内置流程包括细胞复苏、细胞传代、细胞铺板、细胞加刺激、RNA Extraction、Reverse Transcription、SYBR Green qPCR、Western Blot、培养上清收集、ELISA 和 CCK-8。

其中“细胞加刺激”支持同一类型的多个 CELL、PLATE、DISH 或 WELL 输入：CELL/DISH/WELL 以原 Sample 身份登记为输出，PLATE 可按刺激条件生成带 lineage 的 WELL。

## Local-first data and privacy

正式桌面版不依赖 Express 或 localhost HTTP API。Canonical 用户数据与源码目录隔离：

```text
macOS:   ~/Library/Application Support/LabFlow/
Windows: %APPDATA%\LabFlow\
```

目录内包含 `labflow.sqlite` 和 `files/`。图片原件保存在 `files/`，SQLite 只保存附件元数据和相对路径。请使用应用内“数据管理”导出或恢复工作区，不要直接修改数据库文件。

## Desktop and AI Agent architecture

```text
Desktop UI ── Tauri ──┐
                      ↓
               LabFlow Domain/Service
                      ↓
             validation + transaction
                      ↓
                    SQLite
                      ↑
Codex / Agent ─ MCP ──┘
```

Agent 不直接访问 SQLite；MCP 只是 adapter。当前 Agent Interface 已覆盖 Task、Experiment、Protocol 和 Record 的部分操作，所有写入均委托给桌面端共用的 service。详细约束见 [Agent 安装说明](docs/setup/labflow-agent-install.md)和 [架构概览](docs/architecture/overview.md)。

## Download LabFlow

公开下载仓库提供安装包、SHA-256 文件和首次启动说明，不包含源码：

- [Latest macOS and Windows release](https://github.com/echoechofu/labflow-releases/releases/latest)
- [Public installation guide](https://github.com/echoechofu/labflow-releases#readme)
- [Release changelog](https://github.com/echoechofu/labflow-releases/blob/main/CHANGELOG.md)

当前 MVP 尚未经过 Apple Developer ID 公证或 Windows Authenticode 签名。请按照公开安装说明处理 macOS Gatekeeper 或 Windows SmartScreen 提示。

## Development

Requirements: Node.js 24、Rust 1.98、macOS 上的 Xcode Command Line Tools；Windows 安装包由 GitHub Actions 的 Windows runner 原生构建。

```bash
cd eln-app
npm install
npm run tauri:dev
```

工程检查：

```bash
cd eln-app
npm run lint
npm test
npm run build:web
cargo test --manifest-path src-tauri/Cargo.toml
```

更完整的构建、数据隔离和发布说明见 [eln-app/README.md](eln-app/README.md)。

## Documentation

- [产品范围](docs/product/scope.md)
- [用户手册](docs/product/user-guide.md)
- [Protocol domain](docs/domain/protocol.md)
- [Sample lineage](docs/domain/sample-lineage.md)
- [Architecture overview](docs/architecture/overview.md)
- [Database schema](docs/architecture/database.md)
- [Architecture decisions](docs/decisions/)

## Project status

LabFlow 当前处于 MVP 测试阶段，重点服务生物医学 wet-lab 的个人和小团队工作流。当前不包含云同步、多人实时协作、任意 Word/PDF 自动转换为可执行 Protocol，或基于实验数据的自动科学结论。

## License

LabFlow 使用 [PolyForm Noncommercial License 1.0.0](LICENSE)。允许个人、教学、学术研究和其他非商业用途；商业使用需要另行授权。
