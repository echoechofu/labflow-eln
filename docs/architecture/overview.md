# 架构概览

## 正式桌面运行时

```text
React UI
  → @tauri-apps/api invoke
  → Tauri Rust command layer（单连接 Mutex）
  → domain/repository modules
  → SQLite + user filesystem
```

React 通过 `src/repository.ts` 调用 Tauri command；Rust 在应用启动时解析用户数据路径、创建 `files/`、打开 SQLite、应用 schema 演进、确保内置 Protocol，并在数据库为空时写入模板数据。领域写入（Task、Protocol execution、lineage、导出）由 Rust transaction 执行，避免 UI 直接操纵 SQLite。

正式打包的 `.app` 使用 Tauri `frontendDist` 中的静态资源，业务运行时不依赖 Express 或 localhost HTTP API。

## Web 开发兼容层

非 Tauri 环境中，repository 会请求 `/api/store`；`server.ts` 提供 Express + `better-sqlite3` 的开发兼容层，并复用 Node `AppDataPathProvider`。该层用于网页开发、旧数据库迁移和基础测试兼容，不是正式桌面运行时的一部分。新 Sample/lineage 写入命令明确要求 Tauri desktop。

## 本地数据与路径隔离

macOS canonical 用户根目录：

```text
~/Library/Application Support/LabFlow/
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
6. 用户 Protocol：React 三步向导提交受限 Sample Flow 定义；focused Tauri command 在一个 transaction 中注册新 Sample 类型并写入 Protocol v1。模板编辑复制当前 schema 为新的 user version，只替换正文模板。Record 启动仍复用同一 transaction 执行器和 snapshot 机制。
7. 工作区备份：Tauri 用 SQLite Online Backup API 将正在使用的连接复制为一致性快照，再与 `files/` 和 checksum manifest 封装为 `.labflow-backup`。恢复先在 staging 只读校验，并创建当前工作区备份/本地 rollback snapshot；通过后替换式恢复并刷新 UI store。

旧的全量 `save_store` command 仍存在以兼容早期 store 形式，但一旦已有 `process_events` 就拒绝执行，以免覆盖 lineage；当前功能使用 focused commands。
