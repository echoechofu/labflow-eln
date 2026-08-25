# ADR-002: Record 的 Protocol snapshot 与历史正文

## Status

Accepted

## Context

Protocol 会随内置 catalog 升级而产生新 active version。实验记录需要保留创建当时采用的步骤和用户填写的内容，不能在 Protocol 更新后改变历史实验正文。一个 Task 同时也不能重复创建多个 Record。

## Decision

启动 Record 时，执行器在同一 transaction 内从 Protocol 当前 active version 读取 schema，保存其副本到 `records.protocol_snapshot_json`，渲染模板并将正文写入 `records.current_data_json.renderedContent`。`records.task_id UNIQUE` 和执行器对 `task.record_id IS NULL` 的检查共同限制每 Task 一个 Record。

Record 页面与导出读取 Record 自身保存的 snapshot/正文；Protocol 后续的 active version 更新不回写已有 Record。

## Consequences

- 历史 Record 保留创建时的模板内容和字段渲染结果。
- 新 Record 可以使用升级后的内置 Protocol，而既有 Record 不会被模板升级改变。
- export manifest 读取已保存的 Record 内容，能对该次导出计算 hash。
- schema 中存在 `record_changes` 用于保存字段变更审计，但当前没有通用数据库 trigger 阻止 `records` 被 UPDATE，也没有“所有 Record 阶段都建立完整版本链”的机制。故本 ADR 的“不可变”指 Protocol snapshot 与首次渲染正文不被 Protocol 升级改写，不应误读为数据库级的全字段锁定。

## Alternatives considered

- 仅保存 `protocol_id`，显示时读取当前模板：拒绝，历史 Record 会随模板改变。
- 每次读取时重新渲染 snapshot：拒绝，已保存的用户字段/正文不应依赖运行时重新计算。
- 把所有 Record 更新一律禁止：当前未采用；现有 schema 和审计表预留了受控记录变更的空间，但 UI 尚未实现通用编辑流程。
