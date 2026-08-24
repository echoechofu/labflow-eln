import type { Task } from "./domain";

export const TASK_GRAPH_NODE_WIDTH = 210;
export const TASK_GRAPH_NODE_HEIGHT = 104;

export interface TaskGraphNode {
  task: Task;
  level: number;
  order: number;
  x: number;
  y: number;
  connected: boolean;
}

export interface TaskGraphEdge {
  id: string;
  parentId: string;
  childId: string;
  path: string;
}

export interface TaskGraphLayout {
  nodes: TaskGraphNode[];
  edges: TaskGraphEdge[];
  width: number;
  height: number;
  hasCycle: boolean;
  invalidRelationCount: number;
}

const taskOrder = (left: Task, right: Task) =>
  left.start.localeCompare(right.start) || left.id.localeCompare(right.id);

export function buildTaskGraph(tasks: Task[]): TaskGraphLayout {
  const orderedTasks = [...tasks].sort(taskOrder);
  const taskById = new Map(orderedTasks.map((task) => [task.id, task]));
  const relationKeys = new Set<string>();
  const relations: { parentId: string; childId: string }[] = [];
  let invalidRelationCount = 0;

  for (const child of orderedTasks) {
    for (const parentId of child.parentTaskIds || []) {
      const key = `${parentId}\u0000${child.id}`;
      if (
        parentId === child.id ||
        !taskById.has(parentId) ||
        relationKeys.has(key)
      ) {
        invalidRelationCount += 1;
        continue;
      }
      relationKeys.add(key);
      relations.push({ parentId, childId: child.id });
    }
  }

  const children = new Map<string, string[]>();
  const indegree = new Map(orderedTasks.map((task) => [task.id, 0]));
  const levels = new Map(orderedTasks.map((task) => [task.id, 0]));
  const connectedIds = new Set<string>();
  for (const relation of relations) {
    children.set(relation.parentId, [
      ...(children.get(relation.parentId) || []),
      relation.childId,
    ]);
    indegree.set(relation.childId, (indegree.get(relation.childId) || 0) + 1);
    connectedIds.add(relation.parentId);
    connectedIds.add(relation.childId);
  }

  const ready = orderedTasks.filter((task) => indegree.get(task.id) === 0);
  const processed = new Set<string>();
  while (ready.length > 0) {
    ready.sort(taskOrder);
    const task = ready.shift()!;
    processed.add(task.id);
    for (const childId of children.get(task.id) || []) {
      levels.set(
        childId,
        Math.max(levels.get(childId) || 0, (levels.get(task.id) || 0) + 1),
      );
      const nextIndegree = (indegree.get(childId) || 0) - 1;
      indegree.set(childId, nextIndegree);
      if (nextIndegree === 0) ready.push(taskById.get(childId)!);
    }
  }

  const cyclicTasks = orderedTasks.filter((task) => !processed.has(task.id));
  const hasCycle = cyclicTasks.length > 0;
  if (hasCycle) {
    const firstFallbackLevel = Math.max(0, ...Array.from(levels.values())) + 1;
    cyclicTasks.forEach((task, index) =>
      levels.set(task.id, firstFallbackLevel + index),
    );
  }

  const levelsToTasks = new Map<number, Task[]>();
  for (const task of orderedTasks) {
    const level = levels.get(task.id) || 0;
    levelsToTasks.set(level, [...(levelsToTasks.get(level) || []), task]);
  }
  const maximumRows = Math.max(
    1,
    ...Array.from(levelsToTasks.values()).map((items) => items.length),
  );
  const horizontalGap = 92;
  const verticalGap = 34;
  const padding = 36;
  const nodes: TaskGraphNode[] = [];
  for (const [level, levelTasks] of levelsToTasks) {
    levelTasks.sort(taskOrder);
    const centeredOffset =
      ((maximumRows - levelTasks.length) *
        (TASK_GRAPH_NODE_HEIGHT + verticalGap)) /
      2;
    levelTasks.forEach((task, order) =>
      nodes.push({
        task,
        level,
        order,
        x: padding + level * (TASK_GRAPH_NODE_WIDTH + horizontalGap),
        y:
          padding +
          centeredOffset +
          order * (TASK_GRAPH_NODE_HEIGHT + verticalGap),
        connected: connectedIds.has(task.id),
      }),
    );
  }
  const nodeById = new Map(nodes.map((node) => [node.task.id, node]));
  const edges = relations.map((relation) => {
    const parent = nodeById.get(relation.parentId)!;
    const child = nodeById.get(relation.childId)!;
    const startX = parent.x + TASK_GRAPH_NODE_WIDTH;
    const startY = parent.y + TASK_GRAPH_NODE_HEIGHT / 2;
    const endX = child.x;
    const endY = child.y + TASK_GRAPH_NODE_HEIGHT / 2;
    const bend = Math.max(34, Math.abs(endX - startX) / 2);
    return {
      id: `${relation.parentId}-${relation.childId}`,
      parentId: relation.parentId,
      childId: relation.childId,
      path: `M ${startX} ${startY} C ${startX + bend} ${startY}, ${endX - bend} ${endY}, ${endX} ${endY}`,
    };
  });
  const maximumLevel = Math.max(0, ...nodes.map((node) => node.level));
  return {
    nodes,
    edges,
    width: Math.max(
      720,
      padding * 2 +
        (maximumLevel + 1) * TASK_GRAPH_NODE_WIDTH +
        maximumLevel * horizontalGap,
    ),
    height: Math.max(
      300,
      padding * 2 +
        maximumRows * TASK_GRAPH_NODE_HEIGHT +
        (maximumRows - 1) * verticalGap,
    ),
    hasCycle,
    invalidRelationCount,
  };
}
