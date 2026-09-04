# ADR-005: 受限声明式用户 Protocol

## Status

Accepted

## Context

内置 Protocol 的 Record template、Sample 输入输出和 usage 已部分由 JSON execution schema 表达，但孔板、刺激和终末检测仍包含 Rust 专属逻辑。用户需要创建简单 Protocol、注册新的 Sample 类型并修改 Record 正文，同时不能获得执行任意代码或破坏既有 Record/lineage 的能力。

## Decision

用户 Protocol 使用三步创建流程：基本信息、Sample Flow、Record Template。Sample Flow 由受限的 `sample_flow_v1` 执行，支持当前 Experiment 的单一声明输入类型、保留/消耗 usage，以及原 Sample 继续、每输入派生一个、每输入派生多个、按条件组派生多个、measurement-only 输出行为。按条件分配可选顺序孔位映射；孔位作为 metadata 保存，不限制输出 Sample 类型。

Sample 类型通过独立注册表保存；持久化 canonical value 为大写，展示名独立。派生输出默认继承对应父 Sample metadata，并只补充系统 provenance，不要求重新填写 group、stimulus、time，也不把 Record 字段整体复制到 Sample metadata。

用户 Protocol 创建为 v1。修改任一 Protocol 的 Record template 会复制当前 schema、插入新的 user version 并切换 active version；旧 version 和已有 Record snapshot 不改变。内置 catalog 后续同步不能静默替换当前激活的用户版本。

## Consequences

- 简单材料转化、拆分、继续和 measurement-only Protocol 可由用户创建，无需增加 Rust 名称分支。
- 用户不能通过 Protocol 执行 JavaScript、Rust 或 SQL。
- 用户 Protocol 可按条件组生成多个 Sample，并可选映射顺序孔位；图形化手动选孔、终末检测、专属计算和复杂动态字段仍使用内置能力。
- 新类型、Protocol 和 version 都保存在 canonical 用户 SQLite 中，与源代码隔离。

## Alternatives considered

- 为每个用户 Protocol 增加 Rust 专属页面和分支：拒绝，扩展成本高且继续耦合名称。
- 允许用户直接编辑任意 execution JSON 或执行脚本：拒绝，无法维持可验证的数据完整性边界。
- 修改模板时覆盖原 schema：拒绝，会破坏 Protocol version 历史并增加 Record 重解释风险。
