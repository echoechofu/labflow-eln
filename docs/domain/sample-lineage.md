# Sample lineage

## 当前保存方式

Sample lineage 是内部数据完整性模型，而不是当前主 UI 的独立可视化功能。它由以下互补关系保存：

```text
ProcessEvent ── event_inputs ──> Sample
ProcessEvent ── event_outputs ─> Sample
Sample ── sample_relations(derived_from) ─> Sample
Record ── record_samples(input/output) ──> Sample
```

`samples.source_record_id` 记录其创建来源；单一输入派生时 `parent_sample_id` 与 `sample_relations` 记录直接父项。`process_events` 保留过程参数、发生时间和来源；递归查询可从 event 输入/输出追溯上游与关联刺激。

## 创建与使用

启动 Task Record 使用一个 SQLite transaction。对每个执行流程，系统可创建 Record、ProcessEvent、event 输入/输出、Record 输入/输出、输出 Sample、Sample relation、Sample usage 与预置 Result。失败会回滚，避免只留下半套 Record 或 lineage。

已实现的输出行为包括：

| Protocol/场景 | 输入与输出 |
| --- | --- |
| 细胞复苏 | 无既有 Sample 输入；生成一个 `CELL`。 |
| 细胞传代 | 输入 `CELL`；生成 1–96 个 `CELL`，输入标记为 `consumed`。 |
| 细胞铺板 | 输入 `CELL`；生成 `PLATE` 或 `DISH`。Plate metadata 保存受支持规格的容量。 |
| 细胞加刺激 | 输入 `PLATE`、`DISH` 或 `WELL`。Plate 按刺激因素、时间和孔数分配孔位，并生成多个 `WELL`；Dish/Well 代表状态变化，不生成新的输出 Sample。 |
| RNA 提取 | 从直接上级 Task 的 `CELL`/`WELL`/`DISH` 输出中选择一个或多个输入；每个输入生成一个 `RNA`，并消耗输入。 |
| 逆转录 | 从直接上级 Task 的 `RNA` 输出选择；每个输入生成一个 `CDNA`，记录为 `aliquot`。 |
| qPCR | 从直接上级 Task 的 `CDNA` 输出选择；记录 aliquot usage，保存 Targets、Mapping 与 Raw Cq；不创建 Sample，本阶段不创建分析 Result。 |
| Western Blot | 从直接上级 Task 的 `CELL`/`WELL`/`DISH` 输出选择；每个输入生成 `PROTEIN`，并消耗输入，同时创建 `western_blot_image` Result。 |
| 上清收集 | 从直接上级 Task 的 `CELL`/`WELL`/`DISH` 输出选择；每个输入生成 `SUP`，输入为 `non_destructive`。 |
| ELISA | 从直接上级 Task 的 `SUP` 输出选择；记录 aliquot usage，保存 Analytes、Mapping 与 Raw OD；不创建 Sample，本阶段不创建分析 Result。 |
| CCK-8 | 从直接上级 Task 的 `WELL` 输出选择；输入按内置 Protocol 标记为 consumed，保存 Conditions、Mapping 与 Raw OD；不创建 Sample，本阶段不创建分析 Result。 |

孔板分组只允许 6、12、24、48、96 或 384 孔。系统拒绝超过容量、空分组、缺刺激因素/时间的请求，并按行优先方式分配位置。

## 完整性与生命周期

- SQLite trigger 要求 `sample_type` 始终为大写规范值；`CDNA` 在 UI 显示为 `cDNA`。
- `sample_usages` 的部分唯一索引确保一个 Sample 只有一个 `consumed` usage。
- 选择上游产物时，执行器排除已消耗、归档、跨 Experiment 或不属于直接上游 Task 的 Sample。
- 删除 Sample/ProcessEvent 前检查后续 lineage：有下游使用时写 `archived_at`，无下游时才删除相关引用和实体。
- 终末检测的 Sample × AssayItem options、孔位映射和 Raw Measurement 均位于 `assay_*` 表，不写入 `samples`、`sample_relations` 或 `event_outputs`；ProcessEvent 只记录输入与 usage。
- `lineage_status` 可为 `complete`、`partial`、`unknown`；schema 应用会将旧的无输入传代/铺板或无 Plate 输入的 Well 刺激标记为 `partial`。

## 当前边界

当前没有独立、完整的 Sample input/output DSL。通用 execution 字段与 Rust 的事件类型分支共同决定行为；新增 Protocol 不能仅写一个 JSON schema 就保证获得全部 lineage 语义。
