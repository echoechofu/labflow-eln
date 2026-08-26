# 领域模型

## 关系概览

```text
Experiment 1 ── * Task
Task * ── * Task                 （task_relations，depends_on，同一 Experiment）
Task 1 ── 0..1 Record
Protocol 1 ── * ProtocolVersion
SampleType 1 ── * Sample
Record * ── 1 Protocol           （并保存当时的 snapshot）

Record 1 ── * ProcessEvent
Record * ── * Sample             （record_samples：input / output）
ProcessEvent * ── * Sample       （event_inputs / event_outputs）
Sample * ── * Sample             （sample_relations：derived_from）
Record 1 ── * Result / Attachment
Record 1 ── * AssayItem / AssayPlate
AssayPlate 1 ── * WellMapping / RawImport
RawImport 1 ── * RawMeasurement
Record 1 ── * QpcrDeltaCtAnalysis
QpcrDeltaCtAnalysis 1 ── * QpcrDeltaDeltaCtAnalysis
```

## 对象

| 对象 | 当前含义与关系 |
| --- | --- |
| Experiment | 实验工作区。具有唯一 `experiment_code`、标题、说明、颜色；拥有 Task、Sample、ProcessEvent、Container、处理定义与旧版 qPCR 孔位数据。当前终末检测板属于 Record。 |
| Task | 有计划起止时间和状态的实验工作项；属于一个 Experiment，可有多个上级 Task；最多关联一个 Record。 |
| Task relation | `parent_task_id → child_task_id` 的 `depends_on`。父子必须同属 relation 的 Experiment，且不得自环或成环。 |
| Protocol | 内置或用户创建的稳定标识、名称、说明、分类、颜色、来源与 active version。 |
| Protocol version | 同一 Protocol 的版本化 `schema_json`，并记录 builtin/user 来源与创建时间。active version 为新 Record 的来源；历史 Record 不以 active version 作为正文来源。 |
| Sample type | Sample 类型注册项；保存大写 canonical value、展示名称、builtin/user 来源和归档状态。 |
| Record | 单一 Task 的实验记录。保存 `protocol_snapshot_json`、`current_data_json`（标题、字段值、输入/输出 ID、渲染正文等）和更新时间。用户可显式修订单条 Record 的正文，但不会回写 Protocol snapshot 或其他 Record。qPCR Analysis 保存时还会向 `current_data_json.analysisSections` 追加已经渲染完成的分析文字。 |
| Record change | 对 Record 字段的审计行（旧/新 JSON、操作者、时间）。单条正文修订写入 `renderedContent` 变更，qPCR Analysis 追加文字写入 `analysisSections` 变更。 |
| Sample | Experiment 内材料/对象；有 workspace 唯一编号、规范类型、`internal`/`external` 来源、可选来源 Record、可选父 Sample、显示名、metadata、lineage 状态与归档时间。`external` 表示首次登记时已存在于现实世界的 lineage root。 |
| ProcessEvent | 一次材料过程的事件，带 Experiment、可选 Record、时间、参数与来源（`labflow_recorded` / `user_imported`）。 |
| Result | Record 的结构化测量或产物数据；当前新流程中的典型实例是 WB 图像结果。历史 qPCR/ELISA Record 可能保留旧版 pending Result；新版终末检测不为 qPCR Analysis 创建通用 `results` 行，而使用专属分析快照表。Result 不是 Sample。 |
| Attachment | Record 附件的文件名、相对路径、MIME、大小和创建时间；文件实体在用户文件目录。 |
| Export manifest | 一次记录导出的日期范围、Record 集、内容 SHA-256、相对文件路径、状态和创建时间。 |
| Assay item | 终末检测 Record 中的轻量检测项目；qPCR 显示为 Target/Gene，ELISA 为 Analyte，CCK-8 为 Condition。它不是 Sample，也不预先承担内参等分析角色。 |
| Assay plate / Well mapping | Record 的虚拟板与 `Well → Sample × AssayItem` 映射。孔位在同一板内唯一，technical replicate 由相同组合的映射孔数推导。 |
| Raw import / Raw measurement | 原始结果 Attachment 的解析批次，以及 `Well → metric/value`。Mapping 与 measurement 通过同一 plate 和 well 形成 join。 |
| qPCR ΔCt analysis | 对一个 qPCR Record 的基因角色、纳入 measurement、QC 备注和计算结果快照。多个内参不先平均；每个 Target × 内参各自保存一组 ΔCt 结果。 |
| qPCR ΔΔCt analysis | 选择一个已有 ΔCt analysis 和其中一个内参，保存 Sample 分组、共同/对应对照关系及 relative expression 结果快照。 |
| Container / Sample location | 容器及 Sample 的有效时间位置历史；新位置会关闭先前未结束的位置。 |

## 当前不变量

- `experiment_code` 唯一；`(workspace_id, sample_code)` 唯一。
- 一个 Task 至多一个 Record（`records.task_id UNIQUE`）；启动路径同时拒绝已有 `task.record_id` 的 Task。
- Task relation 只允许 `depends_on`，不能自指，且执行层拒绝跨 Experiment、重复父项与循环。
- `sample_type` 持久化为大写规范值；UI 可显示科学写法（例如 `CDNA` 显示为 `cDNA`）。Sample 编号本身保持 Experiment 编号加类型后缀，实验细节进入 metadata。
- 用户 Protocol 激活版本不会被应用启动时的内置 catalog 同步静默覆盖；模板修改创建新 version。
- 一个 Sample 最多有一次 destructive (`consumed`) usage；消耗后不能再作为受限输入。
- Task relation 只表达 workflow 依赖，不限制 Record 的材料输入。输入 Sample 可以来自同一 Experiment 的任意可用 Sample，也可以在启动 Record 的 transaction 中登记为 `external` root。
- Attachment 和 export manifest 在 SQLite 中只保存相对路径，且不得以 `/` 开头。
- Assay Plate 只有在既无 Well Mapping、也无 Raw Import 时才允许删除；删除不会级联清理已有检测数据。
- qPCR Analysis 保存、专属分析快照插入、Record 确切文字追加与 `record_changes` 审计在同一 transaction 内完成。
- 单条 Record 正文修订只更新 `current_data_json.renderedContent` 和 `updated_at`，并在同一 transaction 写入 `record_changes`；Protocol snapshot、Sample lineage 和其他 Record 不变。
- Record 可由详情页显式删除。删除会清理该 Record 自有事件、usage、分析、附件和无下游使用的内部输出 Sample，并将 Task 恢复为 `planned`；若输出已被下游数据使用或 Record 已进入 export manifest，删除被阻止。
- 有下游材料使用的 Sample/ProcessEvent 归档而非删除；没有后续 lineage 时可删除。

上述约束的部分由 SQLite 唯一键、外键、CHECK/trigger 实施，部分由 Rust command/execution 层在 transaction 内校验。并非所有领域规则都有数据库 trigger。
