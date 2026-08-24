# LabFlow

LabFlow 是一个 local-first 的 macOS 实验管理与电子实验记录本（ELN）MVP。它将实验、Task、Protocol、Record、Sample、Result 和附件保存在用户本机 SQLite 数据库中；正式桌面运行时不依赖 Express 或 localhost HTTP API。

## 当前归档版本

本版本已经具备：

- Tauri macOS 桌面应用、开发模式与 `.app` 打包。
- 实验日历：Task 的创建、编辑、删除、状态流转与 24 小时周视图。
- Experiment 内只读 Task 关系图：支持分支、合流与孤立 Task；点击节点可打开既有 Task 详情。
- 内置细胞复苏、细胞传代、铺板、刺激、RNA 提取、逆转录、qPCR、WB、上清液等 Protocol 执行流程。
- Protocol Snapshot：创建 Record 时冻结版本与渲染后的实验正文，后续修改 Protocol 不会改写历史 Record。
- Protocol 驱动的 Sample/Result：支持输入选择、消耗、派生、孔板分孔、Result 与 Sample 分离及内部 lineage 完整性。
- Records 按 Task 的实验日期分组；可选择日期/记录后合并预览，并通过 macOS 系统打印面板保存 PDF。
- 导出清单、附件与 SQLite 均与源码目录隔离。

本归档不包含云同步、多人协作、自由图编辑、Protocol Builder 或任意 Word/PDF 自动转换为可执行 Protocol。

## 架构

```text
React UI
  → Tauri commands
  → local domain / repository
  → SQLite + local filesystem
```

开发网页兼容层可以使用 Express，但打包后的桌面应用使用 Tauri 直接访问本地领域层与 SQLite。

## 用户数据与源码隔离

macOS 的 canonical 用户数据目录固定为：

```text
~/Library/Application Support/LabFlow/
├── labflow.sqlite
└── files/
    └── exports/
```

数据库中的附件和导出文件只保存相对路径，例如：

```text
files/exports/<export-id>/manifest.json
```

**数据文件、附件或运行时用户数据出现在 source/project directory 属于 P0 data-integrity failure。**

## 运行

前置条件：Node.js、Rust stable toolchain、Xcode Command Line Tools。

```bash
cd eln-app
npm install
npm run tauri:dev
```

构建 macOS 应用：

```bash
npm run tauri:build
```

输出位置：

```text
src-tauri/target/release/bundle/macos/LabFlow.app
```

## 工程检查

```bash
npm run lint
npm run build:web
npm test
cargo +1.98.0 check --manifest-path src-tauri/Cargo.toml
cargo +1.98.0 test --manifest-path src-tauri/Cargo.toml
```

当前自验收覆盖：用户数据隔离、fresh migration、旧数据库迁移、重启持久化、样本类型规范化、Task Graph 布局与异常保护、Protocol 执行、Sample lineage、Result 分离、导出清单与冻结 Record 内容。

## 数据模型原则

- Task 关系储存在 `task_relations`，同一 Experiment 内的 `depends_on` 关系构成 DAG。
- Record 保存独立的 Protocol snapshot 与渲染正文；历史记录不引用可变模板。
- Sample 编号保持简洁：Experiment 编号加类型后缀；实验细节存于 metadata。
- Sample lineage 是内部完整性模型，不作为用户主要工作流的可视化对象。
- qPCR、WB 等测量结果写入 `results`，不伪装为 Sample。

## 归档说明

此仓库用于保留当前可运行的 LabFlow MVP 状态。提交时不应包含：

- `node_modules/`
- `dist/`
- `src-tauri/target/`
- `data/`、`*.sqlite`、附件或导出文件
- 用户个人工作区文件
