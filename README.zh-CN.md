# LabFlow ELN：面向生物医学湿实验的电子实验记录本

<p align="center">
  <img src="eln-app/src/assets/hero.png" alt="LabFlow ELN 标志" width="180" />
</p>

<p align="center">
  <strong>安排实验、复用方法、保留真实操作，并把样本和原始文件连接起来。</strong><br />
  面向生物医学湿实验的本地优先电子实验记录本与实验管理桌面软件。
</p>

<p align="center">
  <a href="README.md">English</a> · <a href="README.zh-CN.md">简体中文</a>
</p>

<p align="center">
  <a href="https://github.com/echoechofu/labflow-releases#下载最新版">下载 LabFlow</a>
  ·
  <a href="docs/product/user-guide.md">用户手册</a>
  ·
  <a href="docs/architecture/overview.md">架构说明</a>
</p>

LabFlow 把原本分散在日历、Word、Excel 和文件夹里的实验信息连接成一条工作流：

```text
日历 Task → 带版本的 Protocol → 本次真实 Record → Sample、图片与原始文件
```

它适合希望轻量开始的研究人员、研究生和小型生物医学团队：无需云端账号，也不用先配置一整套实验室 LIMS，就可以从一个真实实验开始记录。

## LabFlow 解决什么问题

### 排期和记录在同一条工作流中

在周日历中安排湿实验 Task，将它们归入 Experiment，并连接前后依赖。Task 可以进入对应的执行 Record，让实验计划和实际发生的证据保持关联。

### 复用方法，同时保留历史事实

使用内置或自建 Protocol，并在不同实验中重复执行。创建 Record 时，LabFlow 会冻结当时的 Protocol 版本快照；以后修改 Protocol 不会改写历史 Record。本次实验的实际参数、操作偏差、观察结果、输入和输出仍然可以单独保存。

### 把实验数据连接起来

登记 Sample 的输入、输出、消耗、派生、处理条件、容器和孔板位置；在 Record 正文中插入实验图片，并附加任意类型的原始文件。SQLite 数据库和附件位于同一本地工作区，也可以一起导出为完整备份。

LabFlow 自带常用生物医学实验 Protocol，用户下载后可以直接从一个真实实验开始，而不必先搭建整套实验室管理系统。

## 产品界面

<table>
  <tr>
    <td colspan="2"><a href="docs/assets/screenshots/weekly-calendar.png"><img src="docs/assets/screenshots/weekly-calendar.png" alt="LabFlow 周日历中的多套生物医学湿实验流程" /></a></td>
  </tr>
  <tr>
    <td colspan="2" align="center"><strong>按周排期</strong><br />在一张日历中持续看到多套跨天湿实验流程。</td>
  </tr>
  <tr>
    <td width="50%"><a href="docs/assets/screenshots/task-editor.png"><img src="docs/assets/screenshots/task-editor.png" alt="LabFlow Task 编辑窗口中的 Experiment、时间和依赖关系" /></a></td>
    <td width="50%"><a href="docs/assets/screenshots/protocol-version-editor.png"><img src="docs/assets/screenshots/protocol-version-editor.png" alt="LabFlow 带版本的 Protocol 编辑器和 Record 预览" /></a></td>
  </tr>
  <tr>
    <td align="center"><strong>设置 Task</strong><br />选择 Experiment、安排时间，并连接前后依赖。</td>
    <td align="center"><strong>带版本的 Protocol</strong><br />更新方法时，已有 Record 继续保留原始快照。</td>
  </tr>
  <tr>
    <td width="50%"><a href="docs/assets/screenshots/record-files.png"><img src="docs/assets/screenshots/record-files.png" alt="LabFlow 实验 Record 中的操作正文和原始实验文件" /></a></td>
    <td width="50%"><a href="docs/assets/screenshots/records-list.png"><img src="docs/assets/screenshots/records-list.png" alt="LabFlow 按实验日期整理的 Record 列表和合并导出" /></a></td>
  </tr>
  <tr>
    <td align="center"><strong>实验文件</strong><br />把操作正文和原始实验文件保存在同一条 Record 中。</td>
    <td align="center"><strong>查看与导出</strong><br />按实验日期查看 Record，并合并导出所选实验记录。</td>
  </tr>
</table>

## 核心工作流

| 工作流 | LabFlow 当前支持 |
| --- | --- |
| 实验排期 | 在 24 小时周日历中创建、编辑、关联和完成 Task |
| 实验上下文 | 将 Task 归入 Experiment、查看只读任务依赖图，并从排期界面隐藏不活跃的 Experiment |
| Protocol 管理 | 使用内置方法或创建自建 Protocol，保留版本历史和 Record 快照 |
| 实验 Record | 记录真实操作、偏差、观察结果、图片和任意类型附件 |
| Sample 追踪 | 登记输入、输出、消耗、派生、实验条件、容器和孔板位置 |
| 终点检测 | 保存 qPCR、ELISA 和 CCK-8 的 Plate Mapping 与 Raw Data 结构 |
| 导出与备份 | 系统打印、低内存 PDF、PDF 加全部原始附件，以及完整工作区备份 |

## 内置生物医学 Protocol

当前目录包括：

- 细胞复苏、传代、铺板和刺激
- RNA 提取与逆转录
- SYBR Green qPCR
- Western Blot
- 培养上清收集
- ELISA 和 CCK-8

自建 Protocol 可以描述原 Sample 沿用、1→1、1→多和 1→0。每条 Record 会登记该次实验实际产生的输出和 Sample lineage。

## 本地优先与数据所有权

正式桌面版不依赖 Express、localhost HTTP API、云端账号或持续联网。Canonical 用户数据位于 App 和源码目录之外：

```text
macOS:   ~/Library/Application Support/LabFlow/
Windows: %APPDATA%\LabFlow\
```

工作区包含 `labflow.sqlite` 和 `files/`。附件原件保存在 `files/`，SQLite 只保存附件元数据和相对路径。请使用 LabFlow 内的“数据管理”导出或恢复完整 `.labflow-backup` 工作区。

## 下载与平台状态

- [macOS 0.1.7 Apple Silicon 版](https://github.com/echoechofu/labflow-releases/releases/tag/v0.1.7)：支持 macOS 12 及以上系统。本版本加入更新签名、启动时检查更新、用户确认下载，以及安装并重启。
- [Windows 0.1.5 x64 版](https://github.com/echoechofu/labflow-releases/releases/tag/v0.1.5)：支持 Windows 10/11。本次 0.1.7 没有更新 Windows 版，Windows 暂不包含应用内更新。
- [公开安装说明](https://github.com/echoechofu/labflow-releases#readme)
- [版本更新记录](https://github.com/echoechofu/labflow-releases/blob/main/CHANGELOG.md)

当前 MVP 的 macOS App 使用 ad-hoc 签名，尚未经过 Apple Developer ID 公证；Windows 安装包尚未使用 Authenticode 签名。如果 Gatekeeper 或 SmartScreen 要求确认，请按照公开安装说明操作。Updater 签名用于验证更新文件的来源和完整性，不能替代操作系统代码签名。

## 可选 MCP 集成

LabFlow 还可以通过内置的 Model Context Protocol（MCP）Server，将部分 Task、Experiment、Protocol 和 Record 能力提供给本地 AI 客户端。桌面界面和 MCP adapter 使用相同的领域服务与校验规则，Agent 不能直接访问 SQLite。

这项集成完全可选；没有 Agent 时，LabFlow 仍然是一套完整的本地桌面实验工作流。详细说明见 [MCP 使用指南](docs/product/mcp-user-guide.md)和[架构概览](docs/architecture/overview.md)。

## 开发

前置条件：Node.js 24、Rust 1.98，以及 macOS 上的 Xcode Command Line Tools。Windows 安装包使用 GitHub Actions 的 Windows runner 原生构建。

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

打包和数据隔离细节见 [eln-app/README.md](eln-app/README.md)。

## 文档

- [产品范围](docs/product/scope.md)
- [用户手册](docs/product/user-guide.md)
- [核心对象、Record 与 Sample Flow](docs/product/core-objects-and-sample-flow-guide.md)
- [Protocol 领域模型](docs/domain/protocol.md)
- [Sample lineage](docs/domain/sample-lineage.md)
- [架构概览](docs/architecture/overview.md)
- [数据库结构](docs/architecture/database.md)
- [架构决策](docs/decisions/)

## 项目状态

LabFlow 当前处于 MVP 测试阶段，重点服务个人研究者和小型生物医学湿实验团队。当前不包含云同步、多人实时协作、任意 Word/PDF 自动转换为可执行 Protocol，或基于实验数据自动生成科学结论。

## 使用许可

LabFlow 使用 [PolyForm Noncommercial License 1.0.0](LICENSE)。允许个人、教学、学术研究和其他非商业用途；商业使用需要另行授权。
