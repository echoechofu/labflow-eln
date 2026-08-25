# SQLite 数据库

## Schema 演进

`src-tauri/src/schema.sql` 提供新数据库的基础表、索引、CHECK 和 trigger。应用启动时 `apply_schema` 还会补加早期数据库缺少的列（例如 Sample metadata、归档/lineage 状态、Task 创建/更新时间），启用外键，并做有限的数据规范化/修复。因此当前有效 schema 是基础 SQL 加启动期的兼容性演进，而非单一静态 SQL 文件。

数据库没有单独的 migration-state 表；schema 通过 `CREATE ... IF NOT EXISTS`、列存在性检查与受限数据更新演进。Node 开发兼容层有较早的最小 schema，用于基础网页兼容和旧库验证；正式桌面以 Rust `apply_schema` 为准。

## 核心表组

| 表组 | 用途 |
| --- | --- |
| `experiments`, `tasks`, `task_relations` | 实验、日程 Task 与同 Experiment 的 depends-on 网络。 |
| `protocols`, `protocol_versions` | 内置 Protocol 标识、活跃版本及 JSON schema。 |
| `records`, `record_changes` | Task 关联的 Record、创建时 Protocol snapshot/渲染正文，以及字段变更审计行。 |
| `samples`, `sample_relations`, `record_samples` | Sample、派生关系和 Record input/output 角色。 |
| `process_events`, `event_inputs`, `event_outputs`, `sample_usages` | 过程事件、材料输入输出及使用语义。 |
| `results`, `attachments` | Record 结果与附件元数据。 |
| `containers`, `sample_locations`, `treatment_definitions`, `qpcr_plate_wells`, `sample_aliases` | 容器/位置历史、刺激定义、qPCR 孔位映射和 Sample 别名。 |
| `entity_changes`, `export_manifests` | 实体级审计与 Record 合并导出清单。 |
| `assay_items`, `assay_plates`, `assay_well_mappings` | 通用终末检测 Setup 与独立孔板映射。 |
| `assay_raw_imports`, `assay_raw_measurements` | 原始文件导入元数据、SHA-256 与按孔解析值。 |

## 关系与约束

- `records.task_id UNIQUE` 使一个 Task 只能有一个 Record。
- `task_relations` 的复合唯一键防止重复边，CHECK 防止自边；Rust 另行实施同 Experiment 和无环校验。
- `sample_relations`、`record_samples`、`event_inputs`、`event_outputs` 使用关联表保存多对多关系。
- `sample_usages` 对 `usage_type='consumed'` 建立部分唯一索引，限制单次破坏性消耗。
- Sample 类型 insert/update trigger 拒绝非大写值。
- `attachments.relative_path`、`export_manifests.relative_path` 的 CHECK 拒绝以 `/` 开头的绝对路径；应用层还要求附件在 `files/` 下。

## SQLite 与用户文件目录的分工

SQLite 保存业务对象、JSON metadata、Record snapshot/正文、审计数据、附件/导出的相对定位符以及 export 内容 hash。二进制附件与导出 manifest 文件保存在 canonical 用户目录的 `files/` 下。manifest 写入成功后才插入数据库元数据；若 insert 失败，代码尝试删除刚创建的 manifest 文件，避免孤立的成功记录。

终末检测 raw 文件也使用 Attachment 相对路径；导入表保存文件 hash、列选择和 metric，measurement 表保存数值/原文本及原行 JSON。`assay_well_mappings` 以 `(plate_id, well_position)` 唯一，raw measurement 以 `(import_id, well_position, metric_key)` 唯一。join 只在读取时连接共同 plate/well，不写回 Sample lineage。

## 历史、覆盖与保留

Protocol 版本不会回写已存在 Record 的 snapshot/正文。Sample 或 ProcessEvent 有下游 lineage 时采用归档（`archived_at`）而不是删除。Record 变更表与实体变更表提供审计存放位置，但当前 schema 没有把所有表更新都强制写入 audit，也没有通用的数据库级“Record 不可 UPDATE”trigger。历史保留的实际强制程度应以各 focused command 的 transaction 和约束为准。
