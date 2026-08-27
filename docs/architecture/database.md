# SQLite 数据库

## Schema 演进

`src-tauri/src/schema.sql` 提供新数据库的基础表、索引、CHECK 和 trigger。应用启动时 `apply_schema` 还会补加早期数据库缺少的列（例如 Sample metadata、归档/lineage 状态、Task 创建/更新时间），启用外键，并做有限的数据规范化/修复。因此当前有效 schema 是基础 SQL 加启动期的兼容性演进，而非单一静态 SQL 文件。

正式 Desktop 与本地 MCP 会打开同一工作区数据库。初始化统一启用 SQLite WAL 和 5 秒 busy timeout，使两个本地进程的读写可以按 SQLite 事务边界协调；领域写入仍必须由 service transaction 完成，不能依赖 WAL 代替业务原子性。

数据库没有单独的 migration-state 表；schema 通过 `CREATE ... IF NOT EXISTS`、列存在性检查与受限数据更新演进。Node 开发兼容层有较早的最小 schema，用于基础网页兼容和旧库验证；正式桌面以 Rust `apply_schema` 为准。

## 核心表组

| 表组 | 用途 |
| --- | --- |
| `experiments`, `tasks`, `task_relations` | 实验、日程 Task 与同 Experiment 的 depends-on 网络。 |
| `protocols`, `protocol_versions` | 内置/用户 Protocol、说明、来源、活跃版本及版本化 JSON schema。 |
| `records`, `record_changes` | Task 关联的 Record、创建时 Protocol snapshot/渲染正文，以及字段变更审计行。 |
| `sample_types`, `samples`, `sample_relations`, `record_samples` | Sample 类型注册、Sample、派生关系和 Record input/output 角色。 |
| `process_events`, `event_inputs`, `event_outputs`, `sample_usages` | 过程事件、材料输入输出及使用语义。 |
| `results`, `attachments` | Record 结果与附件元数据。 |
| `containers`, `sample_locations`, `treatment_definitions`, `qpcr_plate_wells`, `sample_aliases` | 容器/位置历史、刺激定义、旧版 qPCR 孔位数据和 Sample 别名。 |
| `entity_changes`, `export_manifests` | 实体级审计与 Record 合并导出清单。 |
| `assay_items`, `assay_plates`, `assay_well_mappings` | 通用终末检测 Setup 与独立孔板映射。 |
| `assay_raw_imports`, `assay_raw_measurements` | 原始文件导入元数据、SHA-256 与按孔解析值。 |
| `qpcr_delta_ct_analyses`, `qpcr_delta_delta_ct_analyses` | qPCR 两层 Analysis 的配置、计算结果、名称和创建时间快照。 |

## 关系与约束

- `records.task_id UNIQUE` 使一个 Task 只能有一个 Record。
- `task_relations` 的复合唯一键防止重复边，CHECK 防止自边；Rust 另行实施同 Experiment 和无环校验。
- `sample_relations`、`record_samples`、`event_inputs`、`event_outputs` 使用关联表保存多对多关系。
- `sample_usages` 对 `usage_type='consumed'` 建立部分唯一索引，限制单次破坏性消耗。
- Sample 类型 insert/update trigger 拒绝非大写值。
- `sample_types.canonical_type` 为大写主键；Protocol 创建事务使用 `INSERT OR IGNORE` 注册用户类型，避免重复类型行。
- `records.protocol_id` 是冻结 Record 的历史来源标识，不再外键依赖 `protocols`；正式数据迁移会幂等重建旧 `records` 表并执行 `foreign_key_check`。因此用户 Protocol 可以在不破坏历史 Record 的情况下删除。
- `samples.origin` 区分 `internal` 产物与用户登记的 `external` roots。external root 的 `source_record_id`、`parent_sample_id` 为空；内部派生继续使用这些字段、`sample_relations` 和 event link 表。
- Record 启动 transaction 可先插入 external roots，再验证并写入实际 ProcessEvent、usage 和输出；任一步失败时新登记 Sample 也会回滚。
- `attachments.relative_path`、`export_manifests.relative_path` 的 CHECK 拒绝以 `/` 开头的绝对路径；应用层还要求附件在 `files/` 下。

## SQLite 与用户文件目录的分工

SQLite 保存业务对象、JSON metadata、Record snapshot/正文、审计数据、附件/导出的相对定位符以及 export 内容 hash。二进制附件与导出 manifest 文件保存在 canonical 用户目录的 `files/` 下。manifest 写入成功后才插入数据库元数据；若 insert 失败，代码尝试删除刚创建的 manifest 文件，避免孤立的成功记录。

工作区备份不新增业务表。`.labflow-backup` 封装 SQLite Online Backup API 生成的 `database/labflow.sqlite`、完整 `files/` 和 `manifest.json`。manifest 保存备份格式/数据库 schema 版本、App 版本、对象数量、数据库 SHA-256 与逐文件 SHA-256，不保存 canonical 绝对路径。导入只接受 `files/` 下的可迁移 locator，拒绝 path traversal、symlink、未知 entry、更高 schema 版本、不匹配 checksum 或缺失被引用文件的备份。

终末检测 raw 文件也使用 Attachment 相对路径；导入表保存文件 hash、列选择和 metric，measurement 表保存数值/原文本及原行 JSON。`assay_well_mappings` 以 `(plate_id, well_position)` 唯一，raw measurement 以 `(import_id, well_position, metric_key)` 唯一。join 只在读取时连接共同 plate/well，不写回 Sample lineage。通用 parser 支持 UTF-8 CSV/TSV/TXT；qPCR 还可从 XLSX 中定位同时包含 `Well` 与选定 measurement 列的工作表。

qPCR ΔCt table 的 `config_json` 保存目的/内参角色、纳入 measurement ID 和 QC 备注，`result_json` 保存各 Target × 内参的独立结果及冻结文字。ΔΔCt table 引用来源 ΔCt analysis，保存单一内参、Sample 分组、对照模式/关系和 relative expression。分析保存时，同一 transaction 还会向 Record `current_data_json.analysisSections` 追加确切文字，并写 `record_changes`；导出直接携带这份 Record 数据。

## 历史、覆盖与保留

Protocol 模板修改插入新的 user `protocol_versions` 行并切换 active version，不 UPDATE 旧 schema；内置 catalog 同步只在当前 active version 仍为 builtin 时自动切换到更新的 builtin version。用户删除自建 Protocol 时，同一 transaction 删除其全部 version 和主记录；已注册 Sample Type 保留。Protocol 修改或删除都不会回写已存在 Record 的 snapshot/首次渲染正文。用户显式修订单条 Record 正文时，focused command 只更新 `current_data_json.renderedContent` 和 `updated_at`，并在同一 transaction 写入 `record_changes`；不变更 Protocol snapshot 或 Sample lineage。qPCR Analysis 允许在其后追加新的冻结分析文字，但不覆盖旧正文或已有分析 section。Sample 或 ProcessEvent 有下游 lineage 时采用归档（`archived_at`）而不是删除。Record 变更表与实体变更表提供审计存放位置，但当前 schema 没有把所有表更新都强制写入 audit，也没有通用的数据库级“Record 不可 UPDATE”trigger。历史保留的实际强制程度应以各 focused command 的 transaction 和约束为准。

Record 删除由 focused transaction 实施：先检查 export manifest 与输出 Sample 的跨 Record/Event/Mapping 下游引用；通过后按外键顺序删除分析、raw/mapping、事件、usage、Record links、附件元数据和该 Record 产生的内部输出 Sample，并把 Task 恢复为 `planned`。附件目录在 transaction 成功后从 canonical `files/` 中删除。阻断检查失败时不写入任何变化。
