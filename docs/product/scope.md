# LabFlow MVP 范围

本文记录当前仓库中已经实现的 LabFlow 范围；它不是后续功能承诺。实现依据为 `eln-app` 的 React/Tauri 代码、SQLite schema、README 与现有测试。

## 已实现

- macOS local-first 桌面应用：React UI 通过 Tauri command 访问本地 SQLite 和用户文件目录。
- Experiment 管理，以及按 Experiment 归属的 Task 日历；Task 可创建、编辑、删除、开始与完成。日历使用 24 小时周视图。
- Task 可选择同一 Experiment 的多个上级 Task。系统拒绝自依赖、跨 Experiment 依赖、重复父任务和环；有下游依赖或已有 Record 的 Task 不可删除。
- 当前 Mac 上的 Codex 可通过本地 stdio MCP 使用 Calendar/Task、Experiment、Protocol（含用户自建 Protocol 删除）和 Record 基础工具；这些操作与 Desktop 共用 Rust service、validation 和 transaction。Codex 不直接访问 SQLite。
- Experiment 页面可只读展示既有 Task 关系网络（分支、合流、孤立 Task），点击节点打开既有 Task 详情。
- Task 通过“打开记录”进入 Record 启动流程：已有 Record 时打开它；尚无 Record 时选择 Protocol、填写字段和输入 Sample 后创建。
- 内置 Protocol：细胞复苏、细胞传代、细胞铺板、细胞加刺激、RNA 提取、逆转录、qPCR、Western Blot、培养上清收集、ELISA、CCK-8。
- Record 创建时在一个 SQLite transaction 内写入 Record、ProcessEvent、输入/输出 Sample 关联、Sample usage 和预置 Result（如适用）。
- Protocol snapshot 与已渲染的 Record 正文：历史 Record 不会因内置 Protocol 后续升级而被重新渲染。
- Sample lineage、Sample 消耗/非破坏性使用/分装、孔板容量核验与孔位输出；WB 的测量产物建模为 Result，而非 Sample；qPCR、ELISA、CCK-8 本阶段只保存 Setup、Mapping 与 Raw 数据，不创建分析 Result。
- Record 按 Task 开始日期筛选/排序、合并预览，并可生成可校验的 export manifest 后调用系统打印。
- 数据管理可将当前 SQLite 与 `files/` 导出为完整 `.labflow-backup` 工作区备份，或在校验完整性、外键、版本、相对路径与文件 checksum 后替换式恢复。恢复前自动导出当前工作区作为恢复点。
- qPCR、ELISA、CCK-8 的通用终末检测骨架：Record Setup 保存检测项目；独立 Plate Mapping 保存 `Well → Sample × AssayItem`；UTF-8 CSV/TSV 原文件作为 Attachment 保存并解析；仅将 Mapping 与 Raw Measurement 共同存在的孔组成 join dataset。
- SQLite、附件和导出清单与源码目录隔离；macOS canonical 数据目录为 `~/Library/Application Support/LabFlow/`。

## 明确暂不实现

- 云同步、多人协作或远程服务端。
- 两个 LabFlow 工作区的数据合并、部分 Experiment 导入或冲突 ID 改写。
- Record 创建、Sample、Sample lineage、Terminal Assay、qPCR/ELISA/CCK8 Analysis 的 Agent tools；Cloud、Mobile、remote ChatGPT 与多人 Agent 接入。Experiment CRUD、Protocol 模板读写以及 Record 查询/正文修改/删除已通过本地 MCP 提供。
- 打包桌面应用中的 localhost/Express API 依赖。
- Experiment Task Graph 内新增、删除、拖拽或编辑关系；该图只读。
- Protocol Builder、用户上传 Protocol package，或将任意 Word/PDF 自动转换成可执行 Protocol。
- Sample lineage 作为面向用户的主要可视化工作流。
- ELISA 标准曲线、CCK-8 活力归一化等计算，以及相应分析 Result/统计/绘图产出。qPCR 已实现 ΔCt/ΔΔCt 分析快照。
- 通用 XLS/XLSX 或厂商专属二进制 raw parser；当前通用 importer 接受 UTF-8 CSV/TSV/TXT，qPCR 另外支持按列名读取 XLSX。

## 可能扩展、但尚未决定

- 用户自定义 Protocol 的导入、验证、发布方式及其 UI。
- 可声明的、通用的 Protocol/Sample 输入输出 DSL。
- 更完整的 Record 编辑、锁定策略或每次修改的版本化体验。
- Task Graph 的交互式关系编辑、缩放和布局控制。
- 云端备份、协作、权限与跨设备同步。

这些项目不应被视为当前数据库契约或产品承诺。
