# Protocol

## 共享能力与字段编辑（2026-09-12）

创建向导现在提供两条路径：从头配置基础 Sample Flow，或从任意已有 Protocol 的活跃版本创建。后者由共享 `protocol_service` 根据 `sourceProtocolId` 读取并复制 schema；保留来源的 execution、事件类型、metadata 规则、模板变体、Result 和 Terminal Assay 配置。因此 11 个内置 Protocol 的现有执行能力均可成为用户 Protocol 的能力来源，不需要按用户 Protocol ID 增加专用执行分支。

这属于受限能力复用，并非开放任意 execution JSON 或任意组合所有执行规则。铺板、刺激等既有能力内部仍保留已验证的执行分支；未新增手动选孔、ELISA/CCK-8 计算算法或用户脚本执行。

用户可以添加文本、数字和下拉字段，配置标签、必填、默认值、单位、数字范围及基于其他字段值的显隐条件。创建 Record 时由 `ProtocolFields` 渲染，布局交给 `ProtocolLayoutEditors`，状态与提交仍由 `RecordCreationDrawer` 负责。普通字段渲染与样本来源分组分离。

保存 schema 时，共享 `protocol_schema` 校验字段 key、类型、重复键、数字范围和默认值、下拉选项、显隐引用及模板占位符；执行 Record 时按实际输入类型和值检查可见字段、必填项、有限数字和选项合法性。旧 schema 中未声明选项的 select 保留原执行器的专门校验，新建 schema 必须提供选项。

来源能力字段使用服务端 `protectedFieldKeys` 保留其 key、kind、选项、必填和显隐语义，标签与有效默认值可修改；附加普通字段可增删。基础创建的 fields 是附加字段，与系统生成的数量/布局字段合并；来源复制及版本更新的 fields 是完整字段列表。Desktop 与 MCP 都调用同一 service，客户端不能提交 execution 覆盖来源能力。

版本编辑支持字段和正文修改，也可选另一个能力来源。切换来源后新版本使用新来源的事件类型与执行配置；旧 version、Record snapshot、已发生的 ProcessEvent 和渲染正文保持不变。复制所得用户 Protocol 不依赖来源 Protocol 后续存在。

验收通过 11 个内置的 16 个执行对照场景，比较输入输出、正文、metadata、usage、Result、检测项目及刺激历史；另覆盖来源版本切换的实际执行、历史 Record 不变与非法字段值整体回滚。尚未完成新安装包或全平台桌面发布验收。

## 当前模型

Protocol 由 `protocols` 与 `protocol_versions` 表表达。Protocol 和 version 都区分 `builtin` / `user` 来源；活跃版本包含 JSON schema，当前使用的字段包括：

- `blocks`：UI 的步骤概览；
- `fields`：文本、数字、选择、Sample 选择、孔板布局和条件分配字段，以及必填/可见性信息；
- `template` 或 `templateSelector` + `templateVariants`：Record 正文模板；
- `execution`：事件类型、允许的输入类型/来源/基数、输出类型与模式、Sample usage policy、Result 类型。

内置 catalog 当前有 11 个 Protocol：细胞复苏、细胞传代、细胞铺板、细胞加刺激、RNA Extraction — Trizol、Reverse Transcription — PrimeScript、SYBR Green qPCR、Western Blot、培养上清收集、ELISA — 细胞因子、CCK-8 细胞增殖/毒性实验。ELISA 与 CCK-8 的实验正文来自仓库根目录的 `组内protocol整理_Ver1.0.doc`。

从头创建路径可通过三步向导创建 Protocol v1：基本信息、Sample Flow、正文与字段。“适用的输入类型”可选择一种或多种材料；同一 Record 的多个输入仍要求类型一致。真正通用的操作可以显式选择“不限类型”。输出类型不在 Protocol 创建时固定，而由执行者在 Record 中登记实际产出。新建路径强制选择以下身份流转之一：

- 原 Sample 沿用（`same_sample`）：身份不变并固定保留；加刺激等状态变化由 Record 字段描述；
- 1→1（`record_one`）：每个输入恰好登记一个新 Sample，原输入固定消耗；
- 1→多（`record_many`）：每个输入逐行登记一个或多个实际输出，原输入可保留或消耗；
- 1→0（`none`）：原输入固定消耗，不产生输出。

Record 输出清单的一行代表一个真实 Sample，包含来源输入、类型、可选名称、处理方式、处理时间和其他说明。类型按实验对象、组织体液、细胞组分、核酸文库、蛋白小分子和自定义通用类型分组。自定义类型只用于目录缺少某种通用材料类别的情况；肺、鼻黏膜、肝等具体部位统一使用 `TISSUE`，部位写入名称或其他信息。若新建通用类型，显示名与 canonical code 会和 Record 在同一事务中登记。每个输出直接记录对应输入为 parent，并与 Record、ProcessEvent 和 usage 在同一事务中创建。输出清单支持复用上一行。自建 Protocol 首次使用后会询问是否将第一组输入的输出类型序列保存为新版本默认预填，后续仍可逐行修改。

Sample 类型表示材料是什么；处理条件、部位、菌株、细胞系、容器和位置分别保存为属性。新输出清单不提供兜底 `OTHER`，需要目录外材料时应创建有明确名称的自定义类型。孔板、培养皿和孔位不再出现在新输出清单中，但旧 `PLATE`、`DISH`、`WELL` Protocol 和历史 Record 继续按原 snapshot 执行。

1→多的输入 usage 可声明为保留（`non_destructive`）或消耗（`consumed`）；其他三种流转由身份语义固定 usage。持久化类型始终为大写 canonical value，展示名称可保留科学写法。

创建 Record 时，执行器校验必填字段，按任务日期渲染模板，将 schema snapshot 和渲染后的正文写入 Record；随后按 execution rule 创建 ProcessEvent、Sample usage、输出 Sample 或 Result。

qPCR、ELISA、CCK-8 的 schema 还包含当前已实现的 `terminalAssay` 描述：检测项目 UI 名称、raw metric 和允许板型。Record 创建时保存 Assay Items；随后共用独立 Plate Mapping/Raw Data 功能层。这三类 Protocol 不在 Setup 时创建 pending Result。当前只有 qPCR 在共用 join dataset 之后实现了专属 ΔCt/ΔΔCt Analysis；ELISA、CCK-8 尚无计算层。

## 当前专属逻辑与通用边界

`sample_flow_v1` 已为用户 Protocol 提供受限的声明式执行：同一 Experiment 的 Sample 选择或 external Sample 登记、输入类型/基数、普通与条件分配输出行为、usage policy 和父 metadata 继承。它不允许用户提交 JavaScript、Rust 或 SQL。历史 snapshot 中的 `parent_task_outputs` 继续兼容读取，但不再把 Task relation 当作材料合法性的硬约束；当前新建 Protocol 使用 `experiment_samples`。

这仍不是覆盖所有实验能力的完整 DSL。Rust 执行器继续保留内置 Protocol 的专属事件分支，例如：

- `thaw`、`passage`、`plating`、`treatment` 的输入/新建对象校验；
- 孔板刺激分组、支持的孔板规格、容量及孔位分配；
- `one`、`count`、`plate_or_dish`、`plate_wells` 等内置输出模式；
- `cDNA` 的显示后缀与 Sample 类型大写规范化。

因此，基础路径是“受限 Sample Flow Protocol 创建器”；来源路径可复用已有终末检测及 qPCR 专属分析配置。两者都不是任意 Protocol 上传器。可视化手动选孔与未实现的分析算法仍不可配置。

## Protocol 与历史 Record

Protocol 的活跃版本可在启动时随内置 catalog 升级；新的 schema version 会插入 `protocol_versions`。用户可修改任一 Protocol 的 Record template，保存会创建新的 user version，不覆盖旧 version。若当前激活的是用户版本，启动时 catalog 同步不会将其替换为内置版本。

创建的 Record 保存当时 schema 的副本和渲染后正文。历史 Record 的展示和 export 读取自身保存的数据，不重新读取 active Protocol 模板。

用户可以删除自己创建的 Protocol。删除事务移除 `protocols` 主记录及全部 `protocol_versions`，但不会删除创建 Protocol 时注册的 Sample Type，也不会修改任何历史 Record。`records.protocol_id` 只保留为历史标识；Record 名称、版本、Terminal Assay 定义、展示与导出均读取 `protocol_snapshot_json`。内置 Protocol 由 catalog 管理，不允许删除。若旧 Record 的 snapshot 缺少名称或完整 schema，系统会拒绝删除，避免历史功能静默损坏。

## Future design constraints（尚未实现）

- Word/PDF/结构化文件上传导入尚未实现。
- 图形化手动选孔、任意执行规则组合尚未实现。普通字段已可编辑；终末检测通过已有来源配置复用。
- 若扩展通用 schema/导入器，应保持既有 Record snapshot、用户版本和 lineage 数据可读。
