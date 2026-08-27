# ADR-008: 通用 Agent Interface 与本地 MCP

## Status

Accepted

## Context

LabFlow 长期需要同时支持 Desktop UI 与 Codex、ChatGPT 等 Agent，但 Agent 不能获得 SQLite schema 或数据库文件访问权。现有 Desktop Task 路径是 React → Tauri command → Rust → SQLite；Task 写入逻辑此前集中在 command，parent 时间候选规则只存在于 React。

## Decision

LabFlow 建立可扩展的 Agent Interface。MCP 是本地 adapter，不是业务层：tool 只解析结构化输入、调用共享 Rust domain/service、返回结果并映射 domain error。Agent 不直接读取或修改 SQLite。

Tauri 与 MCP 共用 domain service、validation 和 transaction。第一阶段先开放 Calendar / Task，随后在同一模块化 stdio MCP Server 中加入 Experiment、Protocol 与 Record 模块。Task parent relation 由 create/update 原子处理，不增加重复的专用 relation tool。

React 原有的 `parent.start < child.start` 录入规则下沉到 Task service；service 同时保护已有 incoming/outgoing relation，避免通过 MCP 或修改 Task 时间绕过规则。

## Consequences

- Desktop UI 与 Codex 使用同一套 Task 业务规则，不复制 MCP 专用 Task logic。
- MCP 不接受数据库路径，也不公开 schema；本地进程通过 canonical LabFlow workspace adapter 获取数据库。
- Agent Interface 按领域 module 扩展；Task、Experiment、Protocol、Record tools 共用同一 Server 骨架，后续模块继续沿用这一边界。
- Experiment、Protocol（含用户 Protocol 删除）与 Record 基础 tools 已按相同 service 边界接入；Record 创建、Sample lineage、Terminal Assay 与 qPCR/ELISA/CCK8 Analysis tools 仍是后续扩展。
- Cloud、Mobile、remote ChatGPT、multi-user、Supabase 和自动排实验未实现。

## Alternatives considered

- MCP 直接执行 SQL：拒绝，会绕过 domain validation 和 transaction，并向 Agent 暴露持久化细节。
- 复制一套 MCP Task repository：拒绝，Desktop 与 Agent 会产生不一致规则。
- 第一阶段使用 public HTTP/remote server：未采用，本阶段仅需当前 Mac 上的 Codex 本地闭环。
