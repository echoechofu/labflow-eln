# ADR-001: Local-first 用户数据与桌面运行时

## Status

Accepted

## Context

LabFlow MVP 已实现为 macOS 桌面应用；现有 README 和数据完整性政策明确要求 SQLite、附件和其他运行时用户数据不得写入 source/project directory。早期 Node 开发路径曾需要从 `<project>/data/labflow.sqlite` 安全迁移；Tauri 接入后仍需保持已经验证的稳定数据位置，而不能因 bundle identifier 改变用户数据库目录。

## Decision

正式运行时采用 React → Tauri → Rust → SQLite/local filesystem。Tauri 使用平台 `data_dir()` 作为 base，并追加稳定产品目录 `LabFlow`；macOS 解析为 `~/Library/Application Support/LabFlow/`。数据库为 `labflow.sqlite`，文件目录为 `files/`；SQLite 内仅保存可迁移的相对文件路径。

打包应用不依赖 Express/localhost。Node/Express 仅保留为网页开发兼容层和早期迁移支持。若旧项目内数据库存在而 canonical 目标不存在，迁移先复制、再验证 integrity 和所需表；已有目标不覆盖。

## Consequences

- 删除或重建 source build artifacts 不会删除用户数据库。
- 业务层通过路径 abstraction 获取位置，不把绝对 macOS 路径写入业务记录。
- 数据库或附件落到 source/project directory 被定义为 P0 data-integrity failure。
- Tauri 和 Node 需要各自实现路径 adapter，但提供相同的稳定目录语义。

## Alternatives considered

- 将 SQLite 放在 repository 的 `data/`：拒绝，会使源码清理/构建影响用户数据。
- 以 bundle identifier 推导用户根目录：拒绝，会改变已验证的 canonical `LabFlow` 路径。
- 保留长期 localhost API 作为桌面 UI 到 SQLite 的通道：拒绝，正式 Tauri 运行时已可直接通过 command layer 访问本地领域层。
