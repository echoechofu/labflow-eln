# Protocol capability parity：内置能力与自定义能力迁移审计

> 状态：只读审计计划（2026-09-12）。本文记录当前差异，不表示 11 个内置 Protocol 已统一到自定义执行器，也不把“schema 中可描述”误写成“执行器已支持”。
>
> 依据：`docs/domain/protocol.md`、`docs/decisions/ADR-005-declarative-user-protocols.md`、`eln-app/src-tauri/src/protocol_catalog.rs`、`eln-app/src-tauri/src/protocol_execution.rs`、`eln-app/src-tauri/src/protocol_service.rs`、`eln-app/src/ProtocolEditor.tsx`。

## 结论摘要

当前能力是“受限 Sample Flow Protocol 创建器”，而不是任意 Protocol 上传器。用户向导固定生成 `engine: sample_flow_v1`，输入源为 `experiment_samples`、基数为 `many`，只能选择单一输入类型；输出只覆盖 `same_sample`、`per_input`、`per_input_count`、`per_input_conditions`、`none`，usage 只覆盖 `non_destructive`/`consume`，并可选择注册新的 Sample 类型。Record template 是单一字符串；条件组和可选顺序孔位映射是唯一的动态字段扩展。

内置 schema 表面上复用了 `fields`、`template`/`templateSelector`、`execution`，但实际执行器仍按 `eventType` 分支实现 thaw、passage、plating、treatment；`plate_or_dish`、`plate_wells`、孔板容量/位置、内置 metadata 写入和 `resultTypes` 也不等价于自定义能力。`cDNA` 的 sample-code 显示后缀由通用 `next_sample_code` helper 按 canonical `CDNA` 处理，自定义输出同样经过该 helper，不应列为自定义缺口。`terminalAssay` 只在 qPCR、ELISA、CCK-8 三个内置 Protocol 的 Record 创建时创建 Assay Items；它不是自定义可声明能力。qPCR 另有专属 ΔCt/ΔΔCt 分析，ELISA/CCK-8 当前没有计算层。

## 能力基线：自定义 Protocol 实际能做什么

`ProtocolCreationWizard` 的三步是基本信息、Sample Flow、Record Template。服务端把草稿编译成如下受限语义：

| 维度 | 自定义 v1 当前行为 | 与内置的关键差异 |
|---|---|---|
| 输入 | 一个 canonical 类型（持久化大写），Record 时从当前 Experiment 选择/外部登记 Sample；执行器允许 `many` 个，但要求同一 `inputTypes` | 内置可无输入（thaw）、单输入兼容 legacy 解析，或声明多类型；`treatment` 还有 uniform 与孔板专属分支 |
| 输出 | `same_sample`、每输入一个、每输入按数量多个、每输入按条件多个、`none`；自定义 UI 的 derived_* 会映射到上述模式 | 内置还使用 `one`、`count`、`plate_or_dish`、`plate_wells`；这些模式不能从向导创建 |
| usage | `retain`→`non_destructive`，`consume`→`consume`；same_sample+consume 被拒绝 | 内置还使用 `aliquot`；同一消费检查、历史兼容和专属分支仍在 Rust 执行器中 |
| 字段 | 自动生成的 output_count、condition_groups、可选 plate_format；用户正文不产生任意动态字段 | 内置可有 samples、select/number、可见性、plate_layout、更多实验字段；用户不能声明这些复杂字段 |
| 正文 | 单 `template`，可替换 `date`、输入/输出摘要、条件组摘要 | passage 支持 `templateSelector`+`templateVariants`（贴壁/悬浮）；用户向导不能创建 variants，但编辑器能按现有 variants 逐个保存新版本 |
| terminalAssay | 不生成；服务端未开放 terminalAssay 配置 | qPCR/ELISA/CCK-8 启动时创建 Assay Items，随后使用 Plate Mapping/Raw Data |
| lineage/metadata | 输出继承父 Sample metadata，仅补 provenance；不复制 Record 字段整体 | 内置输出还把 values 写入 metadata，并为 PLATE/WELL 增加容量、位置和刺激专属字段 |
| 安全/扩展边界 | 不执行用户 JS/Rust/SQL；事件名为 `custom:<id>`，但仍走同一事务执行器的通用输出逻辑 | 内置名称分支可以拒绝非法输入、解析孔板分组、创建结果、维持旧 snapshot 行为 |

## 11 个内置 Protocol 逐项盘点

下表中的“字段”是 catalog schema 的字段 key/kind/约束；“正文”只标出是否有 variants 及关键模板占位符，不复制完整 SOP。输入/输出以当前 active schema 为准；catalog 的 Rust `schema_version` 与 JSON 内 `schemaVersion` 个别不一致（RNA、RT、WB、上清、ELISA、CCK-8 尤其应在迁移验收中固定快照）。

| 内置 Protocol | schema / 输入 | 输出与消耗 | 字段与正文 variants | terminalAssay | 执行器专属分支/不可由自定义复刻 |
|---|---|---|---|---|---|
| 细胞复苏 `pro-cell-thaw` | JSON v2；无输入类型（无 `inputSource`/`inputCardinality`） | `one`→`CELL`；无 consumptionPolicy | `cell_name:text` 必填；单 `template`，含 date、cell_name | 无 | `thaw` 禁止已有输入；无输入时直接由 `one` 分支创建无 parent 的内部 CELL 输出（不是先登记外部 CELL）；输出 code/display name 规则 |
| 细胞传代 `pro-cell-passage` | JSON v3；未声明 `inputSource`/`inputCardinality`，走 legacy 单输入解析 | `count`→CELL；`consume` | `input_sample:samples`、`cell_name:text`、`culture_mode:select`(贴壁/悬浮，必填)、`output_count:number`必填；`templateSelector:culture_mode`，贴壁/悬浮两 variant | 无 | `passage` 必须 CELL；`count` 1–96；输出标签/父 lineage；自定义只能通过 `per_input_count` 且没有 culture_mode 的专属执行语义 |
| 细胞铺板 `pro-cell-plating` | JSON v4；未声明 `inputSource`/`inputCardinality`，走 legacy 单输入解析 | `plate_or_dish`→PLATE 或 DISH；未声明消耗 | `input_sample:samples`、`cell_name:text`、`container_type:select`必填、`plate_format:select`按孔板可见；单 template | 无 | `plating` 必须 CELL；PLATE 输出必须受支持规格并写 `plate_capacity`；DISH/PLATE 选择由执行器解释。自定义没有 `plate_or_dish`，不能声明容量语义 |
| 细胞加刺激 `pro-cell-treatment` | JSON v5；`experiment_samples`、many、uniform；CELL/PLATE/DISH/WELL | `plate_wells`；non_destructive | `treatment_groups:plate_layout`（仅 PLATE 可见）、`treatment_type:text`（CELL/DISH/WELL 可见）；单 template，含 treatment_summary/plate_layout_summary | 无 | `treatment` 接受四类输入但 uniform 分支拒绝混合类型；PLATE 读取 `metadata_json.plate_capacity` 并按分组顺序分配位置，同时保留旧 `target_wells`；容量超限拒绝；生成 WELL 并写 well_position、treatment_factor/duration/group、source_plate_id。当前没有图形化手动选孔；自定义条件组只生成普通 Sample，不能做 WELL 专属 metadata |
| RNA Extraction — Trizol `pro-rna` | JSON v3（catalog 外层版本 2）；`experiment_samples`、many；CELL/WELL/DISH | `per_input`→RNA；`consume` | `resuspension_volume:number`必填默认20、`storage:select`必填（立即逆转录/-80℃）；单 template，含 input summary、字段 | 无 | 主要走通用 `per_input`，但可接受的三类输入、消耗策略和 RNA 类型注册是内置 schema 约束；自定义可近似 sample flow，但不能从向导表达默认数字/选择字段 |
| Reverse Transcription — PrimeScript `pro-rt` | JSON v3（catalog 外层版本 2）；`experiment_samples`、many；RNA | `per_input`→CDNA；`aliquot` | `rna_amount:number`必填默认1.0、`extra_reactions:number`必填默认2；单 template | 无 | 通用 per-input + aliquot；`next_sample_code` 对 canonical CDNA 显示 `cDNA`，自定义输出同样复用该通用规则。自定义可选 consume/retain，不能声明 aliquot |
| SYBR Green qPCR `pro-qpcr` | JSON v4（catalog 外层版本 3）；`experiment_samples`、many；CDNA | `none`；`aliquot` | `assay_items:text`必填；单 template，含 target/input summary | `{itemLabel: Target / Gene, metricKey: cq, metricLabel: Cq, plateModels:[96,384]}` | 创建 Assay Items；不创建 Sample/Result pending；共用 Plate Mapping/Raw Data；qPCR 专属 ΔCt/ΔΔCt 分析。自定义不能创建 terminalAssay 或进入分析链 |
| Western Blot `pro-wb` | JSON v2（catalog 外层版本 1）；`experiment_samples`、many；CELL/WELL/DISH | `per_input`→PROTEIN；`consume`；`resultTypes:[western_blot_image]` | `target_proteins:text`必填、`gel_percentage:number`必填默认10、`primary_antibody:text`必填、`secondary_antibody:text`必填、`exposure_time:text`；单 template | 无 | 通用 per-input，但 `resultTypes` 会创建 pending `western_blot_image` Result；自定义 schema 没有 resultTypes 入口 |
| 培养上清收集 `pro-supernatant` | JSON v2（catalog 外层版本 1）；`experiment_samples`、many；CELL/WELL/DISH | `per_input`→SUP；`non_destructive` | `collection_time:text`必填、`collection_volume:number`必填、`storage:select`必填（立即检测/-80℃）；单 template | 无 | 通用 per-input；专属之处主要是固定输入/输出类型、字段默认/必填和 SOP。自定义能近似 lineage/usage，但不能用向导配置同样字段 |
| ELISA — 细胞因子 `pro-elisa` | JSON v3；`experiment_samples`、many；SUP | `none`；`aliquot` | `assay_items:text`必填、`sample_dilution:number`必填默认1、`reference_wavelength:select`必填（570/630）；单 template | `{itemLabel: Analyte, metricKey: od450, metricLabel: OD450, plateModels:[96]}` | 创建 Assay Items；共用 Plate Mapping/Raw OD；不创建 pending Result；当前没有 ELISA 计算层。自定义不能声明 terminal assay、波长选项或 Raw OD 数据契约 |
| CCK-8 细胞增殖/毒性实验 `pro-cck8` | JSON v2（catalog 外层版本 1）；`experiment_samples`、many；WELL | `none`；`consume` | `assay_items:text`必填、`assay_mode:select`必填、`cell_count:number`必填、`culture_volume:number`必填默认100、`cck8_volume:number`必填默认10、`incubation_time:number`必填默认1、`reference_wavelength:text`默认650；单 template | `{itemLabel: Condition, metricKey: od450, metricLabel: OD450, plateModels:[96]}` | 创建 Assay Items；共用 Plate Mapping/Raw OD；不创建 pending Result；当前无计算层。输入必须 WELL 且 consume；自定义不能保证 WELL 输入、终末检测和这些实验字段 |

## 实际执行器分支地图

迁移时要以执行器行为而不是 catalog 文案作为契约：

1. 所有 Record 都先校验 required fields、选择 template/variant、写入 protocol snapshot 和渲染正文；有 `terminalAssay` 才按 `assay_items` 创建 Assay Items。
2. `inputSource` 为 `experiment_samples` 或 legacy `parent_task_outputs` 时走统一输入解析：Experiment 归属、未 archived、未 consumed、允许类型、many/one 基数及 uniform 类型检查。未声明这些字段的 thaw/passage/plating 走 `resolve_or_create_input`：thaw 可无输入，passage/plating 可从 `cell_name` 登记 CELL，treatment 可登记 PLATE/DISH/WELL。
3. event type 校验只对 `thaw`、`passage`、`plating`、`treatment` 有名字专属约束。`treatment` 还解析 PLATE 的布局并保留旧 snapshot 的显式 positions/`target_wells` 回退路径。
4. output mode 分支是第二层专属面：`one`、`count`、`per_input`、`per_input_count`、`per_input_conditions`、`plate_or_dish`、`plate_wells`、`same_sample`、`none`。`plate_wells` 自动产生 WELL 及位置/刺激 metadata；`plate_or_dish` 校验支持的孔板规格；条件输出可按每个输入展开，位置只作为 metadata。
5. 非 `sample_flow_v1` 输出会把 Record values 扩展进 Sample metadata；声明式用户 flow 只继承父 metadata，再补 provenance。所有输出均写 source record/protocol/version；canonical `CDNA` 的代码显示后缀由通用 helper 处理，用户 flow 也会复用。
6. `resultTypes` 只按内置 schema 创建 pending Result；terminal assay 本身不创建 pending Result。这两种机制不要在迁移中合并成一个“结果输出”概念。

## 对比后的迁移分层

### 可直接迁移到通用 Sample Flow 的部分

RNA Extraction、培养上清收集以及（忽略字段/usage 精度时）RT、Western Blot 的“多输入→每输入一个输出”可以映射到 `per_input`。简单的 measurement-only Record 可映射到 `none`。简单拆分可映射到 `per_input_count`；条件分配可映射到 `per_input_conditions`，孔位仅应作为 metadata。

### 需要先补通用 schema/执行器，不能直接迁移的部分

细胞复苏需要无输入并直接创建内部 CELL；细胞铺板需要 `plate_or_dish`、受支持规格与 `plate_capacity`；细胞加刺激需要内置的分组顺序分配、旧 positions 兼容及 WELL 专属 metadata（当前没有图形化手动选孔）；RT 的 aliquot、WB 的 resultTypes、qPCR/ELISA/CCK-8 的 terminalAssay/Plate Mapping/Raw Data，以及 qPCR 分析都超出自定义向导 v1。

## 迁移验收案例（必须以 snapshot 与数据库结果验收）

每个案例都应同时检查 Record 的 `protocol_snapshot_json`、渲染正文、ProcessEvent、`sample_usages`、`event_inputs/event_outputs`、lineage 与 metadata；失败输入应确认事务回滚，不留下半成品 Record/Assay/Sample。

| ID | 场景与输入 | 期望验收 |
|---|---|---|
| M01 | 自定义 `sample_flow_v1`：CELL×2，`per_input_count`，count=3，non_destructive | 产生 6 个目标类型 Sample，每个 parent 正确、metadata 继承且含 source provenance；输入各一条 non_destructive usage；正文含输出摘要；无事件名专属分支依赖 |
| M02 | 自定义条件组：CELL×2，2 条件×2 replicate，开启 plate mapping、96 孔板 | 产生 8 个输出；条件/replicate/group/plate_position 按每个输入展开；输出类型不被改成 WELL；输入不被 consume；超出 96 位置时失败且事务无残留 |
| M03 | same_sample + consume（自定义及手工伪造 schema 各一次） | 向导保存和执行器均拒绝；数据库无 Record、usage、output 残留 |
| M04 | passage：CELL 输入，culture_mode=贴壁 与 悬浮各一条，output_count=2 | 两次正文分别取正确 variant；各产生 2 个 CELL，消耗输入；缺 output_count、0、97 都拒绝；历史 snapshot 的 schema/version 原样可读 |
| M05 | thaw：无 supplied input，仅 cell_name | 直接创建无 parent 的内部 CELL、`thaw` ProcessEvent 和一个 CELL 输出；提供已有输入必须拒绝；迁移不能把它强行改成“必须选择 Experiment Sample” |
| M06 | plating：CELL 输入，孔板/96 与培养皿各一条 | 孔板输出为 PLATE 且 metadata.plate_capacity=96；培养皿输出为 DISH；不支持规格拒绝；两者均保留 CELL parent lineage |
| M07 | treatment：两块不同容量 PLATE，group layout 各自分配；另测混入 CELL | uniform 校验拒绝混合 CELL/PLATE 输入；逐板 `plate_capacity` 校验与分组顺序布局摘要正确；输出 WELL 带 position/factor/duration/group/source_plate；旧 snapshot 仅有 target_wells 时仍能读取执行，并检查 derived_treatments 可见 |
| M08 | RNA extraction：CELL/WELL/DISH 输入→RNA；随后 RT 用 RNA→CDNA | RNA extraction consume；RT aliquot；CDNA canonical type 为 CDNA、sample code 显示后缀为 cDNA（该后缀为通用 helper 行为，自定义也应保留）；不能把 aliquot 静默迁移成 consume 或 non_destructive |
| M09 | qPCR：CDNA×2、多个 targets、96/384 plate model | 创建 terminal Assay Items；不创建 Sample 或 pending Result；Raw Cq 可关联 Record；qPCR 分析入口/ΔCt/ΔΔCt 仍可用；自定义同模板执行不应声称具备该能力 |
| M10 | ELISA：SUP、多个 analytes、参考波长 570/630；CCK-8：WELL、Conditions、增殖/毒性 | 各自创建正确 terminal items、板型限制正确、无 pending Result；ELISA/CCK-8 计算层仍为空；CCK-8 输入 consume，ELISA aliquot |
| M11 | WB：多输入、required antibody/gel 字段、exposure_time 可空 | 每输入 PROTEIN、consume；创建一个 `western_blot_image` pending Result；缺任一 required 拒绝；自定义迁移不得丢掉 resultTypes 语义 |
| M12 | 版本/历史回归：编辑内置 passage variant 或用户 template，重启/删除用户 Protocol 后查看旧 Record | 保存生成新 user version；旧 Record 使用自身 snapshot/template/terminalAssay 展示与导出；catalog 同步不替换 active user version；删除用户 Protocol 不删 Sample Type、不改历史 Record；旧 `parent_task_outputs` snapshot 仍可读 |

M07 还必须验证刺激历史查询：`lineage::derived_treatments` 当前按 `process_events.event_type='treatment'` 查询。若将刺激流程迁移为 `custom:<id>` 而只保留 `plate_wells` 的输出形状，WELL 的上游刺激历史会消失；这不是可接受的“等价”。应先决定事件类型别名/规范化查询的兼容方案，再迁移。

## 必须保留的历史行为

- Record 永远保存 schema snapshot 与渲染正文；展示/export 不重新读取 active Protocol。版本编辑必须复制成新 user version。
- 内置 Protocol 由 catalog 管理、不可删除；用户 Protocol 可删，但不能删除其注册 Sample Type 或修改历史 Record。删除前若旧 snapshot 不完整应拒绝删除。
- `parent_task_outputs` 旧 snapshot 继续兼容读取；新建用户 Protocol 使用 `experiment_samples`，不能把旧 Task relation 重新变成材料合法性的硬约束。
- Sample type 持久化 canonical 大写，展示名独立；`CDNA` 的 sample code 显示 `cDNA` 后缀必须保持。
- 输出 Sample 继承父 metadata，并增加 provenance；条件输出的 plate position 是附加 metadata，不把输出类型强制变成 WELL。
- `same_sample` 与 consumed 的矛盾组合必须在服务层和执行器层都拒绝；已 consumed 输入、重复选择、错误 Experiment/类型/孔板容量也必须维持现有拒绝文案语义或至少维持失败边界。
- treatment 旧显式孔位/`target_wells` snapshot 回退、plate capacity 校验、WELL 专属 provenance/metadata、qPCR/ELISA/CCK-8 的 terminal assay 保存和 qPCR 分析入口都不能因“统一”而静默消失。
- `lineage::derived_treatments` 对 `event_type='treatment'` 的查询结果属于历史可见性契约；custom event 若不被该查询兼容，不能宣称已迁移。必要时须保留 canonical treatment event type 或让读取端兼容旧/新类型。
- 字段显隐目前主要在 `RecordCreationDrawer.tsx` 前端按 `visibleWhen`/`visibleForInputTypes` 控制，而 Rust `validate_required_fields` 只检查 `required` 字段非空，不统一执行显隐条件。迁移/强化 schema 校验前，必须决定后端是否补齐条件校验，并保留现有历史 snapshot 的可读性。

## 风险分级：轻模型可交付 vs 必须统筹

### 可交给轻模型的低风险工作

轻模型可以做 catalog/schema 字段盘点、11 项表格的机械校对、字段 key 与 JSON version 的一致性检查、补充测试矩阵草稿、核对文档链接和 snapshot/export 的只读回归清单。它也可以实现不改变执行语义的文档、类型注释或针对既有行为的快照测试。

### 必须由统筹 agent 评审/决策的风险

涉及 `protocol_execution.rs` 分支合并、`sample_flow_v1` 扩展、terminalAssay/Plate Mapping/Raw Data 数据契约、qPCR ΔCt/ΔΔCt、WELL/PLATE lineage 与 metadata、`lineage::derived_treatments` 的 treatment event 查询、`resultTypes` 生命周期、schema version 迁移、旧 snapshot/`parent_task_outputs` 兼容、Protocol 删除与 catalog 同步、后端显隐/required 语义的工作，必须由统筹 agent 设计并评审。它们会改变历史数据解释、材料消耗或结果可追溯性，不能按“字段相同”自动迁移。

特别是“把 11 个内置都改成 sample_flow_v1”不是本文件的结论或已完成事项；在没有逐案例通过 M01–M12、旧 Record 导出对照、失败事务回滚与真实 terminal/分析回归前，应继续明确标注为未统一。
