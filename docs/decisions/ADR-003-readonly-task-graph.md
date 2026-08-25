# ADR-003: Experiment Task Graph 第一版只读

## Status

Accepted

## Context

Task 之间已有持久化的 `task_relations`，并且同一 Experiment 内允许分支与合流。用户需要在 Experiment 页面理解现有工作关系，但第一版没有必要在可视化图中新增另一套关系编辑入口。

## Decision

Experiment 页面只读取 Task 和 `parentTaskIds` 构建左到右的 DAG 布局，显示分支、合流和孤立 Task。图中点击节点仅打开已有 Task 详情。图本身不新增、删除、拖拽或修改 relation，不写 SQLite。

Task relation 的创建和更新仍在 Task 创建/编辑保存流程中执行，并由 command 层验证同 Experiment、无重复、自环或循环。

## Consequences

- 图是现有数据库关系的展示，不会成为第二个可能产生不一致的写入口。
- 无关系的 Task 仍可见，异常或循环输入可被安全标示，而不使页面崩溃。
- 关系编辑能力若未来加入，需要单独设计交互、事务和冲突处理；本 ADR 不把只读限制定义为永久产品限制。

## Alternatives considered

- 在第一版图中支持拖拽连线编辑：未采用，关系写入和校验已在 Task 编辑流程，图交互会扩大本轮范围。
- 只显示线性 Task 列表：未采用，不能表达现有的分支和合流关系。
