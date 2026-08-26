# ADR-006: External Sample roots

## Status
Accepted

## Context

用户可能在现实实验已经进行到中途时开始使用 LabFlow。此时当前材料没有 App 内祖先记录，但仍需要作为后续 Record 的真实输入。原执行路径要求材料来自直接上级 Task 输出，把 workflow 依赖和实际材料使用绑定在一起，导致无上级 Task 的 root Task 无法准确登记输入。

## Decision

Sample 使用 `origin = internal | external` 明确来源。external Sample 是同一 Experiment 中有意声明的 lineage root，没有来源 Record、父 Sample或伪造的 Import Task/Event。Record 启动 transaction 允许选择 Experiment 已有可用 Sample，或登记 external Samples 并立即作为输入。

Task relation 继续只表达 workflow 依赖。Sample 输入的合法性由 Experiment、类型、归档和 consumption 状态决定，不由是否属于直接上级 Task 决定。一个 Experiment 可以拥有多个 lineage roots。

## Consequences

- 中途接入不需要补造历史 Task 或 ProcessEvent。
- external root 之前的现实历史不被推测；LabFlow 从登记时开始记录后续 lineage。
- external Sample 没有父项是正常边界，因此保持 `lineage_status = complete`。
- 登记、Record、实际 ProcessEvent、usage 和输出在同一 transaction 中提交或回滚。
- 历史 Protocol snapshot 中的 `parent_task_outputs` 继续兼容，但不再构成材料输入的硬约束。

## Alternatives considered

- 创建假的 Import Task：会污染 Task Graph，并把数据登记伪装成实验步骤。
- 为 external Sample 创建假的 import ProcessEvent：会让 event output 看起来像 App 内实验产物。
- 继续要求直接上级 Task 输出：无法支持中途接入，也错误地把日程依赖等同于材料来源。
