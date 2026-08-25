# 当前核心工作流

## 实验与任务

```text
Experiment
  └─ Task（可有多个同 Experiment 的上级 Task）
       ├─ 开始 Task / 更新状态
       └─ 打开记录
```

创建或编辑 Task 时，用户可选择已有 Experiment，或在同一 transaction 内新建 Experiment 并创建 Task。Task 的上级关系保存在 `task_relations`，语义为 `depends_on`。关系在保存 Task 时校验为同一 Experiment 内无环图。

Task 状态当前为 `planned`、`in_progress`、`completed`。Task 详情统一提供“打开记录”：是否已有 Record 只决定后续动作，而不改变入口文案。

## 从 Task 启动 Record

```text
尚无 Record 的 Task
  → 选择内置 Protocol
  → 填写 Protocol 字段 / 选择允许的输入 Sample
  → 单一 SQLite transaction
  → Record + ProcessEvent + input/output Sample + usage + Result（如需要）
  → Task 关联 Record，并成为 in_progress
```

- 同一 Task 最多关联一个 Record：`records.task_id` 为唯一值，执行层也只接受 `record_id IS NULL` 的 Task。
- 需要上游产物的 Protocol 只允许选择当前 Experiment 中、**直接上级 Task** 的 Record 输出，且类型必须符合 Protocol execution rule；已消耗或已归档的 Sample 不可选。
- 细胞复苏不接受既有输入 Sample。传代、铺板和刺激可按各自规则选择输入，或在允许时创建用户导入的起始对象。

## Protocol 到历史 Record

```text
内置 Protocol 的 active version
  → 创建时复制 schema 至 Record.protocol_snapshot_json
  → 用任务日期与用户字段渲染正文
  → 写入 Record.current_data_json.renderedContent
```

之后内置 Protocol 版本升级会更新 `protocols.active_version`，仅影响后续启动的 Record；已有 Record 的正文由其自己的 `current_data_json.renderedContent` 读取，不会回读或重渲染活跃模板。

## Sample、过程与结果

```text
Record
  └─ ProcessEvent
       ├─ event_inputs / record_samples(role=input)
       ├─ event_outputs / record_samples(role=output)
       ├─ sample_usages（consumed / non_destructive / aliquot）
       └─ results（当前如 WB；旧版 qPCR / ELISA Record 可保留历史 Result）
```

例如，传代消耗输入 Cell 并生成一个或多个新的 Cell；铺板生成 Plate 或 Dish；对 Plate 加刺激按分组生成 Well；RNA 提取对每个输入生成 RNA；逆转录生成 cDNA。qPCR、ELISA、CCK-8 进入终末检测数据骨架，不生成 Sample，本阶段也不生成分析 Result。完整规则见 [Protocol](../domain/protocol.md) 与 [Sample lineage](../domain/sample-lineage.md)。

## 终末检测 Mapping 与 Raw Data

```text
Record Setup：输入 Samples + Assay Items
  → 新建 Assay Plate
  → Plate Mapping：Well → Sample × AssayItem

同一 Assay Plate
  ← Raw Result Attachment：Well → Measurement

Mapping INNER JOIN Raw Measurement（plate + well）
  → Mapped Join Dataset
```

Mapping 和 raw upload 没有强制顺序。原始 UTF-8 CSV/TSV/TXT 文件保存为 Record Attachment，解析值保存在 SQLite；未映射的 raw well 不进入 join dataset。technical replicate 只在 UI 中按同一映射组合的孔数派生显示，不持久化重复序号。本阶段 join dataset 只用于核对，不执行计算或创建分析 Result。

## 按日期导出 Record

Records 页面按关联 Task 的 `start_time` 日期整理，而非按 `records.updated_at` 或独立的 performed-at 字段。用户选择日期范围及 Record 后，系统读取 Record snapshot、正文、关联 Sample/Result/Attachment，按 Task 开始时间排序，生成 JSON export manifest 与 SHA-256。manifest 位于用户数据目录的 `files/exports/<export-id>/manifest.json`，随后 UI 调用系统打印能力。
