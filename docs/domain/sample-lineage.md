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

`samples.origin` 区分两种起点：`internal` 由 LabFlow Record / ProcessEvent 产生；`external` 是用户首次登记时已经存在于现实世界的 Sample。external Sample 是有意声明的 lineage root，没有来源 Record、父 Sample 或伪造的 Import Task/Event。一个 Experiment 可以有多个互不关联的 external roots。

## 创建与使用

启动 Task Record 使用一个 SQLite transaction。用户可选择同一 Experiment 的已有可用 Sample，或登记一个/多个 external Sample；后者会与 Record、ProcessEvent、input usage、输出 Sample 和 lineage 一起提交或一起回滚。Task relation 仅用于 workflow，不决定材料输入是否合法。

已实现的输出行为包括：

| Protocol/场景 | 输入与输出 |
| --- | --- |
| 细胞复苏 | 无既有 Sample 输入；生成一个 `CELL`。 |
| 细胞传代 | 输入 `CELL`；生成 1–96 个 `CELL`，输入标记为 `consumed`。 |
| 细胞铺板 | 输入 `CELL`；生成 `PLATE` 或 `DISH`。Plate metadata 保存受支持规格的容量。 |
| 细胞加刺激 | 输入 `PLATE`、`DISH` 或 `WELL`。Plate 按刺激因素、时间和孔数分配孔位，并生成多个 `WELL`；Dish/Well 代表状态变化，不生成新的输出 Sample。 |
| RNA 提取 | 从 Experiment 的 `CELL`/`WELL`/`DISH` 中选择或登记输入；每个输入生成一个 `RNA`，并消耗输入。 |
| 逆转录 | 从 Experiment 的 `RNA` 中选择或登记输入；每个输入生成一个 `CDNA`，记录为 `aliquot`。 |
| qPCR | 从 Experiment 的 `CDNA` 中选择或登记输入；记录 aliquot usage，保存 Targets、Mapping、Raw Cq 与专属 ΔCt/ΔΔCt 分析快照；不创建 Sample，也不创建通用分析 Result。 |
| Western Blot | 从 Experiment 的 `CELL`/`WELL`/`DISH` 中选择或登记输入；每个输入生成 `PROTEIN`，并消耗输入，同时创建 `western_blot_image` Result。 |
| 上清收集 | 从 Experiment 的 `CELL`/`WELL`/`DISH` 中选择或登记输入；每个输入生成 `SUP`，输入为 `non_destructive`。 |
| ELISA | 从 Experiment 的 `SUP` 中选择或登记输入；记录 aliquot usage，保存 Analytes、Mapping 与 Raw OD；不创建 Sample，本阶段不创建分析 Result。 |
| CCK-8 | 从 Experiment 的 `WELL` 中选择或登记输入；输入按内置 Protocol 标记为 consumed，保存 Conditions、Mapping 与 Raw OD；不创建 Sample，本阶段不创建分析 Result。 |
| 用户 Sample Flow Protocol | 从 Experiment 选择或登记已声明类型的输入；可声明原 Sample 继续、每个输入派生一个、每个输入派生多个或 measurement-only，并分别选择保留/消耗输入。 |

孔板分组只允许 6、12、24、48、96 或 384 孔。系统拒绝超过容量、空分组、缺刺激因素/时间的请求，并按行优先方式分配位置。

## 完整性与生命周期

- SQLite trigger 要求 `sample_type` 始终为大写规范值；`CDNA` 在 UI 显示为 `cDNA`。
- `sample_types` 注册表保存内置与用户定义类型的 canonical value、展示名和来源；用户创建 Protocol 时可在同一 transaction 中注册新类型。
- `sample_usages` 的部分唯一索引确保一个 Sample 只有一个 `consumed` usage。
- 选择输入时，执行器排除已消耗、归档、跨 Experiment 或类型不匹配的 Sample；是否属于直接上级 Task 不再是材料约束。
- external Sample 保持 `lineage_status=complete`，表示“已知谱系边界从登记时开始”，而不是缺失数据错误。
- 删除 Sample/ProcessEvent 前检查后续 lineage：有下游使用时写 `archived_at`，无下游时才删除相关引用和实体。
- 终末检测的 Sample × AssayItem options、孔位映射和 Raw Measurement 均位于 `assay_*` 表，不写入 `samples`、`sample_relations` 或 `event_outputs`；qPCR ΔCt/ΔΔCt 分析同样只读取 join dataset 并保存分析快照，不产生新的 lineage 实体。ProcessEvent 只记录输入与 usage。
- `lineage_status` 可为 `complete`、`partial`、`unknown`；schema 应用会将旧的无输入传代/铺板或无 Plate 输入的 Well 刺激标记为 `partial`。
- `sample_flow_v1` 的派生 Sample 默认复制对应父 Sample 的 metadata，并补充来源 Sample、Record、Protocol 和 version；Record 表单值不会整体复制进 Sample metadata。

## 当前边界

当前有受限的 `sample_flow_v1`，但没有覆盖孔板、终末检测和任意动态字段的完整 Sample input/output DSL。内置专属 execution 分支仍是当前模型的一部分。
