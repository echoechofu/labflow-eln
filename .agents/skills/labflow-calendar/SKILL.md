---
name: labflow-calendar
description: Use the LabFlow MCP to view, create, inspect, reschedule, update, or delete experiment calendar Tasks. Apply to LabFlow scheduling and Task-status requests; do not use it for Protocols, Records, Samples, Assays, analyses, or direct database access.
---

# LabFlow Calendar interaction contract

Use the `labflow` MCP as the only interface to live LabFlow data. Never read or modify LabFlow's SQLite database or application-data files directly.

The current Agent Interface covers Experiments and calendar Tasks only:

- `labflow_list_experiments`
- `labflow_list_tasks`
- `labflow_get_task`
- `labflow_create_task`
- `labflow_update_task`
- `labflow_delete_task`

If the user requests Protocol, Record, Sample, lineage, Assay, result, or analysis operations, explain that the current MCP does not expose them. Do not approximate those operations with Task tools.

## Time semantics

Treat all MCP datetimes as local calendar values in the user's current timezone. Send them as `YYYY-MM-DDTHH:mm:ss` without a UTC offset.

Resolve relative dates such as “today” and “tomorrow” from the host-provided current date and timezone. In the response, include the resolved calendar date when that prevents ambiguity.

For a day query, call `labflow_list_tasks` with local midnight as `range_start` and the following midnight as the exclusive `range_end`. The tool returns Tasks overlapping the interval, not only Tasks whose start falls inside it.

## Read workflow

Use `labflow_list_tasks` for calendar queries and `labflow_get_task` when the user refers to one Task or needs its Record link or parents. Use `labflow_list_experiments` when Experiment names are needed to interpret or present Task results.

Sort results chronologically. Present the local date/time, Task title, Experiment title, and status. Do not expose internal IDs unless they help disambiguate or the user asks for them. Never infer that a Task is completed merely because its scheduled end time has passed.

## Create workflow

Before every `labflow_create_task` call, call `labflow_list_experiments` and resolve a valid `experiment_id`.

A create request needs a Task title, start, end, and Experiment:

- Use an Experiment only when the user named it or the active conversation identifies exactly one Experiment unambiguously.
- Do not infer the Experiment from a Task title, an Experiment description, or a plausible scientific workflow.
- If more than one Experiment could apply, show the concise candidates and ask the user to choose before writing.
- If the end is omitted but the user supplies a bounded time range such as “7–8”, use that range. Otherwise ask for the missing time rather than inventing a duration.
- Add `parent_task_ids` only when the user explicitly identifies the dependency or the conversation establishes it unambiguously. Resolve every parent through MCP data; never invent an ID.

Report the Task returned by the create call. Treat that returned object—not the requested inputs—as the committed result.

### Bootstrapping an empty workspace

New users (or users returning to a workspace that has been reset) may have
**zero Experiments and zero parent Tasks**. `labflow_create_task` requires a
valid `experiment_id`, so the bootstrap path is mandatory when
`labflow_list_experiments` returns an empty array.

1. After calling `labflow_list_experiments` and seeing an empty result,
   explain that LabFlow needs at least one Experiment before a Task can be
   created. Do not invent the Experiment's `id`, `code`, or `title` — show
   the user a suggested `code` and `title` based on what they just told
   you (e.g. the Task's experiment context) and ask for confirmation.
2. Once the user confirms the Experiment fields, call
   `labflow_save_experiment` with `experiment: { id, code, title,
   description?, color? }` and `changed_at` set to the current local
   datetime. The service creates-or-updates in one shot; no separate "create
   empty" step exists.
3. Re-call `labflow_list_experiments` (or trust the `save_experiment`
   response) to confirm the new `experiment_id`.
4. Now call `labflow_create_task` with that `experiment_id`.

For the Task itself, an empty workspace also means **no candidate parent
Tasks** exist. `parent_task_ids` defaults to `[]` and that is the correct
shape for a top-level Task. Do not invent parent IDs, do not run a second
`list_tasks` to "find" parents, and do not require the user to specify a
parent when none exist.

The same bootstrap applies any time the user asks for a Task inside an
Experiment they have not yet named — always check
`labflow_list_experiments` first.

## Update workflow

Resolve the target Task from MCP data and call `labflow_get_task` before updating it. If multiple Tasks match the user's wording, ask which one they mean.

Send only fields the user intends to change. In particular, omitting `parent_task_ids` preserves existing parents, while `parent_task_ids: []` removes all parents. Do not change status implicitly when moving or renaming a Task. Valid statuses are `planned`, `in_progress`, and `completed`.

After the update, report the values returned by LabFlow, including any unchanged context needed to make the result clear.

## Delete workflow

Resolve and inspect the exact Task before deletion. A direct request to delete one unambiguous Task authorizes that deletion; otherwise ask for clarification before calling `labflow_delete_task`. Never broaden a deletion request to other Tasks.

LabFlow protects Tasks that have a Record or downstream dependencies. If deletion is rejected, preserve the Task and explain the returned conflict. Do not remove dependencies or records as a workaround unless the user separately requests an exposed, supported operation.

## Errors and authority

LabFlow is authoritative for Experiment existence, datetime validation, parent timing, cross-Experiment relations, DAG cycles, deletion guards, and transaction outcome.

Surface `validation_error`, `not_found`, `conflict`, and `persistence_error` in plain language. Do not claim a mutation succeeded without a successful MCP result. Do not retry a mutation after an uncertain transport or persistence failure until its outcome has been checked with a read tool; this prevents duplicate writes.
