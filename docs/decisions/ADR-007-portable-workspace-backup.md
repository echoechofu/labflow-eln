# ADR-007: 可迁移工作区备份采用替换式恢复

## Status

Accepted

## Context

LabFlow 的本地工作区同时包含 SQLite 中的 Experiment、Task、Record、Protocol、Sample lineage 和分析数据，以及 `files/` 中的附件与导出文件。目标用户试用和 App 迭代需要一个不依赖源码目录、可在安装与设备之间携带的完整数据边界。

两个工作区中的 ID、Task DAG、Record snapshot、Sample/ProcessEvent lineage、attachment locator 和终末检测映射彼此关联。未定义冲突规则时直接合并会破坏这些不变量。

## Decision

导出生成带专用扩展名的 `.labflow-backup` ZIP 容器，包含 SQLite 一致性快照、`files/` 和带版本/数量/checksum 的 manifest。数据库快照使用 SQLite Online Backup API，不直接复制可能带 WAL 状态的运行中文件。

导入为完整替换式恢复，不合并两个工作区。备份必须先在 staging 通过路径安全、格式/schema version、SQLite integrity/foreign keys、核心表、对象数量、relative locator 与 checksum 校验。用户确认后，系统先导出当前工作区到 canonical `backups/`，同时保留本次操作的数据库/文件 rollback snapshot。恢复或启动期 schema 演进失败时，两类用户数据一起回滚。

## Consequences

- 备份不依赖 bundle identifier 或原机器绝对路径，并保留 Protocol snapshot、Record 修订历史、Sample lineage、Mapping/Raw Data 与附件。
- 导入是明确的高影响操作；UI 必须先显示已校验摘要并要求用户确认，不能静默覆盖。
- 正常 App 升级仍继续使用 canonical `LabFlow/` 数据目录，不要求每次更新手动导入。
- 该决策不定义云同步、自动上传、部分 Experiment 恢复或工作区合并语义。

## Alternatives considered

- 直接复制运行中的 `labflow.sqlite`：拒绝，无法保证 WAL/页状态的一致性。
- 导入时按表合并：未采用，当前没有跨工作区 ID、DAG、lineage 与文件冲突的可验证规则。
- 只备份 SQLite：拒绝，Attachment/Raw Result/export manifest 将指向不存在的用户文件。
