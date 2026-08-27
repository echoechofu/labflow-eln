# Protocol

## 当前模型

Protocol 由 `protocols` 与 `protocol_versions` 表表达。Protocol 和 version 都区分 `builtin` / `user` 来源；活跃版本包含 JSON schema，当前使用的字段包括：

- `blocks`：UI 的步骤概览；
- `fields`：文本、数字、选择、Sample 选择和孔板布局字段，以及必填/可见性信息；
- `template` 或 `templateSelector` + `templateVariants`：Record 正文模板；
- `execution`：事件类型、允许的输入类型/来源/基数、输出类型与模式、Sample usage policy、Result 类型。

内置 catalog 当前有 11 个 Protocol：细胞复苏、细胞传代、细胞铺板、细胞加刺激、RNA Extraction — Trizol、Reverse Transcription — PrimeScript、SYBR Green qPCR、Western Blot、培养上清收集、ELISA — 细胞因子、CCK-8 细胞增殖/毒性实验。ELISA 与 CCK-8 的实验正文来自仓库根目录的 `组内protocol整理_Ver1.0.doc`。

用户可通过三步向导创建 Protocol v1：基本信息、Sample Flow、Record Template。当前用户定义范围是单一输入类型和以下四种输出语义：

- 原 Sample 继续（`same_sample`）；
- 每个输入派生一个新 Sample（`per_input`）；
- 每个输入按 Record 启动时填写的数量派生多个 Sample（`per_input_count`）；
- 仅检测、不产生 Sample（`none`）。

输入 usage 可声明为保留（`non_destructive`）或转化/消耗（`consumed`）；系统拒绝“原 Sample 继续 + consumed”的矛盾组合。用户也可注册新的 Sample 类型。持久化类型始终为大写 canonical value，展示名称可保留科学写法。

创建 Record 时，执行器校验必填字段，按任务日期渲染模板，将 schema snapshot 和渲染后的正文写入 Record；随后按 execution rule 创建 ProcessEvent、Sample usage、输出 Sample 或 Result。

qPCR、ELISA、CCK-8 的 schema 还包含当前已实现的 `terminalAssay` 描述：检测项目 UI 名称、raw metric 和允许板型。Record 创建时保存 Assay Items；随后共用独立 Plate Mapping/Raw Data 功能层。这三类 Protocol 不在 Setup 时创建 pending Result。当前只有 qPCR 在共用 join dataset 之后实现了专属 ΔCt/ΔΔCt Analysis；ELISA、CCK-8 尚无计算层。

## 当前专属逻辑与通用边界

`sample_flow_v1` 已为用户 Protocol 提供受限的声明式执行：同一 Experiment 的 Sample 选择或 external Sample 登记、输入类型/基数、四种输出行为、usage policy 和父 metadata 继承。它不允许用户提交 JavaScript、Rust 或 SQL。历史 snapshot 中的 `parent_task_outputs` 继续兼容读取，但不再把 Task relation 当作材料合法性的硬约束；当前新建 Protocol 使用 `experiment_samples`。

这仍不是覆盖所有实验能力的完整 DSL。Rust 执行器继续保留内置 Protocol 的专属事件分支，例如：

- `thaw`、`passage`、`plating`、`treatment` 的输入/新建对象校验；
- 孔板刺激分组、支持的孔板规格、容量及孔位分配；
- `one`、`count`、`plate_or_dish`、`plate_wells` 等内置输出模式；
- `cDNA` 的显示后缀与 Sample 类型大写规范化。

因此，当前能力是“受限 Sample Flow Protocol 创建器”，不是任意 Protocol 上传器。孔板布局、终末检测、专属计算和复杂字段仍需已实现的内置 schema/执行器支持。

## Protocol 与历史 Record

Protocol 的活跃版本可在启动时随内置 catalog 升级；新的 schema version 会插入 `protocol_versions`。用户可修改任一 Protocol 的 Record template，保存会创建新的 user version，不覆盖旧 version。若当前激活的是用户版本，启动时 catalog 同步不会将其替换为内置版本。

创建的 Record 保存当时 schema 的副本和渲染后正文。历史 Record 的展示和 export 读取自身保存的数据，不重新读取 active Protocol 模板。

用户可以删除自己创建的 Protocol。删除事务移除 `protocols` 主记录及全部 `protocol_versions`，但不会删除创建 Protocol 时注册的 Sample Type，也不会修改任何历史 Record。`records.protocol_id` 只保留为历史标识；Record 名称、版本、Terminal Assay 定义、展示与导出均读取 `protocol_snapshot_json`。内置 Protocol 由 catalog 管理，不允许删除。若旧 Record 的 snapshot 缺少名称或完整 schema，系统会拒绝删除，避免历史功能静默损坏。

## Future design constraints（尚未实现）

- Word/PDF/结构化文件上传导入尚未实现。
- 用户为自定义 Protocol 增加任意动态 Record 字段、孔板/终末检测能力尚未实现。
- 若扩展通用 schema/导入器，应保持既有 Record snapshot、用户版本和 lineage 数据可读。
