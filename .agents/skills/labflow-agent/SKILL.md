---
name: labflow-agent
description: Authoritative LabFlow Agent interaction contract. Apply to any LabFlow-related agent request (Codex, ChatGPT, workbuddy, or other MCP clients) before invoking LabFlow MCP tools. Enforces the call chain, the agent-side boundaries, and the Protocol-vs-Record distinction. Module-specific workflows live in companion skills such as labflow-calendar.
---

# LabFlow Agent interaction contract

LabFlow exposes a local `labflow` MCP server. Every agent client — Codex,
ChatGPT, workbuddy, or anything else that can speak MCP — interacts with the
running LabFlow workspace **only** through these tools. Do not read or modify
LabFlow's SQLite database, its `~/Library/Application Support/LabFlow/` files,
or any record inside the application-data root.

> **Installing this combo on another machine?** Run
> `./scripts/install-labflow-mcp.sh` from the repo root. It builds the
> release binary, copies the umbrella + calendar skills into
> `~/.workbuddy/skills/`, registers the server in `~/.workbuddy/mcp.json`,
> and registers it with `codex mcp` when the Codex CLI is on `PATH`.
> Manual steps (Linux / Windows / custom MCP clients) live in
> `docs/setup/labflow-agent-install.md`.

## Authoritative call chain

The architecture is fixed and not negotiable from the agent side:

```text
MCP tool
  → shared Rust domain/service (e.g. task_service)
  → validation
  → SQLite transaction
  → persistence
```

- `src-tauri/src/agent_interface/` is an *adapter*. It only declares
  structured tool schemas, dispatches into the shared service, and maps
  domain errors.
- The shared service (Tauri commands and MCP tools both reach it) owns
  validation and the SQLite transaction. Desktop UI and Agents see the same
  business rules.
- Agents must never:
  - Read or write the SQLite database directly.
  - Open or write to the canonical user-data directory.
  - Re-implement domain rules on the agent side and bypass the service.
  - Approximate an unsupported operation by chaining Task tools.

If a capability is not yet exposed as a tool, tell the user and stop. Do not
guess.

## Workspace bootstrap (no Experiments, no parent Tasks)

New users — or users returning to a freshly reset workspace — start with
zero Experiments and zero Tasks. `labflow_create_task` requires a valid
`experiment_id`, so the agent must bootstrap the workspace before any Task
write.

Required flow when the user asks to create a Task in a workspace that may
be empty:

1. Call `labflow_list_experiments` first. If the result is non-empty,
   proceed with the normal `labflow_create_task` path (resolve the
   Experiment, optionally resolve parents via `labflow_list_tasks`).
2. If `labflow_list_experiments` returns an empty array, do **not** invent
   an `experiment_id`. Explain to the user that LabFlow needs an
   Experiment first. Propose a `code` and `title` derived from the
   conversation, then ask the user to confirm (or amend) those fields.
3. Call `labflow_save_experiment` with `experiment: { id, code, title,
   description?, color? }` and `changed_at` set to the current local
   datetime. The service performs insert-or-update in a single shot.
4. Use the returned (or freshly-read) `experiment_id` and call
   `labflow_create_task` with `parent_task_ids: []`.

Rules that apply while the workspace is empty:

- `parent_task_ids: []` is the correct shape for a top-level Task. Do not
  require parents, do not invent IDs, do not run `labflow_list_tasks`
  searching for them.
- Never re-use `experiment_id` from another user's workspace or guess a
  UUID-style id; `labflow_save_experiment` expects `id` to be a stable
  caller-chosen string. Show the proposed id alongside code and title.
- `labflow_save_experiment` is a single insert-or-update call; there is no
  separate "create empty experiment" tool. Don't chain through `delete` to
  reach a creation state.

The same bootstrap applies any time the agent is about to write a Task,
Record, or anything else that depends on an `experiment_id` — always check
`labflow_list_experiments` first.

## Determining input sample type before `labflow_create_protocol`

Before calling `labflow_create_protocol`, the agent must reason about the
**canonical input sample type** and obtain explicit user confirmation. Never
default to whatever literal name appears in the protocol body (e.g. seeing
"96 孔板" does **not** mean the input type is `PLATE`).

Required workflow:

1. Read the protocol description end to end. Identify the smallest unit the
   experiment actually manipulates — the cell, well, sample, RNA, etc. The
   container (plate, tube, dish) is rarely the Sample; the contents of the
   container are.
2. From that reasoning, propose 2–3 candidate canonical types in the
   user's language (CELL / PLATE / WELL / RNA / CDNA / PROTEIN / SUP / etc.),
   each with a one-sentence reason it could fit. Do **not** embed builtin
   examples or sample ids in this list — keep the candidate set focused on
   the proposed Protocol only.
3. Show the user the candidates and your recommended pick. Ask which one
   they want before any MCP call.
4. Only after the user confirms the canonical type, build the request and
   call `labflow_create_protocol`.
5. Persist the confirmed canonical type in your reasoning trail so future
   versions of the same Protocol stay consistent unless the user revisits
   this step.

Hard rules:

- Never let the literal name of a piece of equipment decide the input type.
- Never re-use the input type of an existing Protocol just because it looks
  similar; re-do the reasoning per Protocol.
- Never combine a literal container name (plate / dish / tube) with a
  derived Sample type (RNA / CDNA / etc.) in the same protocol —
  `outputBehavior: per_input` / `per_input_count` is the path for derived
  Samples.

## Module composition

`LabFlowMcp::compose_module_routers` is the only place where modules merge.
The Task module is the first one registered; future modules — Protocol,
Record, Sample lineage, Terminal Assay, qPCR/ELISA/CCK8 Analysis — will plug
in there. When you add a tool for any module, the tool must delegate to the
shared Rust service; do not duplicate business logic in the agent layer or
in a second MCP service.

## Tools exposed today

Four Agent Interface modules are wired into `compose_module_routers`. Each
tool delegates to the same shared Rust domain service that backs the Desktop
UI's Tauri commands — no duplication, no shortcuts.

### Calendar / Task module (labflow-calendar companion skill)
- `labflow_list_experiments`
- `labflow_list_tasks`
- `labflow_get_task`
- `labflow_create_task`
- `labflow_update_task`
- `labflow_delete_task`

### Experiment module
- `labflow_get_experiment`
- `labflow_save_experiment` — lineage audit row appended automatically.
- `labflow_delete_experiment` — refused when tasks, samples, or lineage history still reference the Experiment.

### Protocol module
- `labflow_list_protocols` — every Protocol template (built-in and user-defined) with the active version's schema summary.
- `labflow_get_protocol`
- `labflow_create_protocol` — user-defined Protocol at version 1; validates the template body and registers new input/output Sample types.
- `labflow_save_protocol_version` — appends a schema version and promotes it to active.
- `labflow_delete_protocol` — permanently deletes a user-defined Protocol and all template versions. Built-ins are protected; complete historical Record snapshots and registered Sample types are retained.

Before `labflow_delete_protocol`, resolve the exact Protocol with
`labflow_get_protocol` and state that all template versions will be removed.
Only call it after an explicit user deletion request. Existing Records are not
a blocker when their snapshots are complete; never delete Records or Sample
types as a workaround for a Protocol deletion conflict.

### Record module
- `labflow_list_records` — optionally filtered by Experiment.
- `labflow_get_record` — full summary (inputs / outputs / results / attachments / history).
- `labflow_update_record_body` — empty bodies are rejected; a change audit row is appended automatically.
- `labflow_delete_record` — refuses when the Record is part of an export manifest or when output Samples are reused downstream.

Calendar-specific workflows (time semantics, Experiment resolution, parent
semantics, deletion guards) live in `labflow-calendar`. Load that skill for
Task operations.

Sample lineage, Terminal Assay, qPCR / ELISA / CCK8 Analysis are not yet
exposed. If the user asks about them, surface that the current MCP does not
provide them — do not emulate them with Task tools.

## Error contract

Tools return structured errors with a stable `code` and a human-readable
`message`:

| `code` | When LabFlow uses it | What to do |
| --- | --- | --- |
| `validation_error` | Bad input (empty title, end ≤ start, malformed datetime, bad datetime range, malformed Protocol template, empty Record body). | Fix the request and retry. |
| `not_found` | Referenced Experiment / Task / Protocol / Record does not exist. | Resolve IDs through MCP reads. Never invent IDs. |
| `conflict` | Parent time later than child, cross-Experiment parent, cycle, duplicate parent, deletion guard (Task has Record or downstream Tasks, Experiment has dependents, Record in export manifest, built-in Protocol deletion, incomplete legacy Record snapshot), duplicate Protocol id, etc. | Explain the conflict in plain language and ask the user how to proceed. |
| `persistence_error` | SQLite / lock / serialization failure. | Treat as uncertain transport; verify with a read before any retry to avoid duplicate writes. |

LabFlow is authoritative for Experiment existence, datetime validation,
parent timing, cross-Experiment relations, DAG cycles, deletion guards, and
transaction outcome. Do not contradict the returned error.

## Time semantics

All datetimes are local calendar values in the user's current timezone. Send
them as `YYYY-MM-DDTHH:mm[:ss]` without a UTC offset. Resolve relative dates
such as "today" and "tomorrow" from the host-provided current date and
timezone, and surface the resolved date when ambiguity would otherwise hide.

## Protocol vs Record — they are not the same thing

LabFlow keeps two different things that the user may casually call
"protocol":

- **Protocol** (the schema/template): lives in `protocols` and
  `protocol_versions`. It is the canonical, versioned schema for running an
  experiment. The user-facing folder in this repository — for example
  `组内protocol整理_Ver1.0.doc` — is *guidance* for operators. Guidance
  documents can change freely; they do not produce any data.
- **Record** (the actual experimental record): created from a Task by
  `start_task_record`. A Record freezes a snapshot of the Protocol at the
  moment of creation, renders the body from that snapshot, and then becomes
  the source of truth for that experiment. Subsequent edits to the Protocol
  template never rewrite a saved Record.

Consequences for an agent:

- Never answer a Protocol-guidance question with "the Record says …". The
  Record reflects a snapshot taken at one moment; the guidance document may
  have been updated since.
- Never suggest modifying a Record by editing the Protocol template.
- When the user asks about "the protocol", clarify whether they mean the
  guidance document (read-only, no MCP) or a saved Record (per-Task). The
  current Record MCP supports lookup, body updates, and guarded deletion;
  Record creation remains a Desktop workflow.

## What the agent must never do

- Open, copy, edit, or query the SQLite file or any file under
  `~/Library/Application Support/LabFlow/`.
- Invoke a tool the MCP does not expose, or invent a tool name.
- Loop a mutation after an uncertain transport or persistence failure
  without first verifying the prior outcome with a read tool. The service
  commits atomically; a second write without verification can duplicate
  data.
- Treat a successful MCP read as proof that a previous mutation succeeded;
  reads are not transaction logs.
- Promise a Sample lineage, Assay, or Analysis operation through
  Task / Experiment / Protocol / Record tools. The contract is that those
  modules will be added in their own time, with their own tools.

## Summary

The MCP is a thin, module-extensible adapter over the shared domain
service. Tools → service → validation → transaction → SQLite, full stop.
Agents stay outside that chain and rely on the structured tool surface; the
Desktop UI sees the same data through the same service.
