# 领域模型

## 关系概览

```text
Experiment 1 ── * Task
Task * ── * Task                 （task_relations，depends_on，同一 Experiment）
Task 1 ── 0..1 Record
Protocol 1 ── * ProtocolVersion
Record * ── 1 Protocol           （并保存当时的 snapshot）

Record 1 ── * ProcessEvent
Record * ── * Sample             （record_samples：input / output）
ProcessEvent * ── * Sample       （event_inputs / event_outputs）
Sample * ── * Sample             （sample_relations：derived_from）
Record 1 ── * Result / Attachment
Record 1 ── * AssayItem / AssayPlate
AssayPlate 1 ── * WellMapping / RawImport
RawImport 1 ── * RawMeasurement
```

## 对象

| 对象 | 当前含义与关系 |
| --- | --- |
| Experiment | 实验工作区。具有唯一 `experiment_code`、标题、说明、颜色；拥有 Task、Sample、ProcessEvent、Container、处理定义与 qPCR 孔位映射。 |
| Task | 有计划起止时间和状态的实验工作项；属于一个 Experiment，可有多个上级 Task；最多关联一个 Record。 |
| Task relation | `parent_task_id → child_task_id` 的 `depends_on`。父子必须同属 relation 的 Experiment，且不得自环或成环。 |
| Protocol | 当前为内置模板的稳定标识、名称、分类、颜色与 active version。 |
| Protocol version | 同一 Protocol 的版本化 `schema_json`。active version 为新 Record 的来源；历史 Record 不以 active version 作为正文来源。 |
| Record | 单一 Task 的实验记录。保存 `protocol_snapshot_json`、`current_data_json`（标题、字段值、输入/输出 ID、渲染正文等）和更新时间。 |
| Record change | 对 Record 字段的审计行（旧/新 JSON、操作者、时间）。表存在并在兼容存储写入路径中保存；当前 UI 没有通用 Record 编辑器。 |
| Sample | Experiment 内材料/对象；有 workspace 唯一编号、规范类型、来源 Record、可选父 Sample、显示名、metadata、lineage 状态与归档时间。 |
| ProcessEvent | 一次材料过程的事件，带 Experiment、可选 Record、时间、参数与来源（`labflow_recorded` / `user_imported`）。 |
| Result | Record 的结构化测量或产物数据；当前新流程中的典型实例是 WB 图像结果。历史 qPCR/ELISA Record 可能保留旧版 pending Result；新版终末检测改存 `assay_*` Setup/Mapping/Raw 数据，本阶段不创建分析 Result。Result 不是 Sample。 |
| Attachment | Record 附件的文件名、相对路径、MIME、大小和创建时间；文件实体在用户文件目录。 |
| Export manifest | 一次记录导出的日期范围、Record 集、内容 SHA-256、相对文件路径、状态和创建时间。 |
| Assay item | 终末检测 Record 中的轻量检测项目；qPCR 显示为 Target/Gene，ELISA 为 Analyte，CCK-8 为 Condition。它不是 Sample，也不预先承担内参等分析角色。 |
| Assay plate / Well mapping | Record 的虚拟板与 `Well → Sample × AssayItem` 映射。孔位在同一板内唯一，technical replicate 由相同组合的映射孔数推导。 |
| Raw import / Raw measurement | 原始结果 Attachment 的解析批次，以及 `Well → metric/value`。Mapping 与 measurement 通过同一 plate 和 well 形成 join。 |
| Container / Sample location | 容器及 Sample 的有效时间位置历史；新位置会关闭先前未结束的位置。 |

## 当前不变量

- `experiment_code` 唯一；`(workspace_id, sample_code)` 唯一。
- 一个 Task 至多一个 Record（`records.task_id UNIQUE`）；启动路径同时拒绝已有 `task.record_id` 的 Task。
- Task relation 只允许 `depends_on`，不能自指，且执行层拒绝跨 Experiment、重复父项与循环。
- `sample_type` 持久化为大写规范值；UI 可显示科学写法（例如 `CDNA` 显示为 `cDNA`）。Sample 编号本身保持 Experiment 编号加类型后缀，实验细节进入 metadata。
- 一个 Sample 最多有一次 destructive (`consumed`) usage；消耗后不能再作为受限输入。
- Attachment 和 export manifest 在 SQLite 中只保存相对路径，且不得以 `/` 开头。
- 有下游材料使用的 Sample/ProcessEvent 归档而非删除；没有后续 lineage 时可删除。

上述约束的部分由 SQLite 唯一键、外键、CHECK/trigger 实施，部分由 Rust command/execution 层在 transaction 内校验。并非所有领域规则都有数据库 trigger。
