import type { Experiment, Task, TaskStatus } from "./domain";
import { dayLabel, formatTime } from "./domain";
import {
  TASK_GRAPH_NODE_HEIGHT,
  TASK_GRAPH_NODE_WIDTH,
  type TaskGraphLayout,
} from "./taskGraph";

const PADDING = 46;
const HEADER_HEIGHT = 126;
const FOOTER_HEIGHT = 34;
const MAX_CANVAS_EDGE = 8192;
const MAX_CANVAS_PIXELS = 16_000_000;

export interface ExperimentGraphPngInput {
  experiment: Experiment;
  graph: TaskGraphLayout;
  subtitleByTaskId: Record<string, string>;
  dateRange?: string;
}

export const experimentGraphPngFileName = (experiment: Experiment) => {
  const identity = `${experiment.code || experiment.title || "Experiment"}`
    .trim()
    .replace(/[\\/:*?"<>|]+/g, "-")
    .replace(/\s+/g, "-")
    .slice(0, 80);
  return `LabFlow-${identity || "Experiment"}-Task-Network.png`;
};

export function experimentGraphCanvasSize(graph: TaskGraphLayout) {
  const width = Math.max(900, graph.width + PADDING * 2);
  const height = HEADER_HEIGHT + graph.height + FOOTER_HEIGHT;
  const scale = Math.min(
    2,
    MAX_CANVAS_EDGE / width,
    MAX_CANVAS_EDGE / height,
    Math.sqrt(MAX_CANVAS_PIXELS / (width * height)),
  );
  return {
    logicalWidth: width,
    logicalHeight: height,
    pixelWidth: Math.max(1, Math.floor(width * scale)),
    pixelHeight: Math.max(1, Math.floor(height * scale)),
    scale,
  };
}

const statusColor: Record<TaskStatus, string> = {
  planned: "#aaaabb",
  in_progress: "#efad4f",
  completed: "#4bab7b",
};

function roundedRect(
  context: CanvasRenderingContext2D,
  x: number,
  y: number,
  width: number,
  height: number,
  radius: number,
) {
  const r = Math.min(radius, width / 2, height / 2);
  context.beginPath();
  context.moveTo(x + r, y);
  context.arcTo(x + width, y, x + width, y + height, r);
  context.arcTo(x + width, y + height, x, y + height, r);
  context.arcTo(x, y + height, x, y, r);
  context.arcTo(x, y, x + width, y, r);
  context.closePath();
}

function fittedText(
  context: CanvasRenderingContext2D,
  value: string,
  maximumWidth: number,
) {
  if (context.measureText(value).width <= maximumWidth) return value;
  let result = value;
  while (
    result.length > 1 &&
    context.measureText(`${result}…`).width > maximumWidth
  )
    result = result.slice(0, -1);
  return `${result}…`;
}

function drawLegend(context: CanvasRenderingContext2D, x: number, y: number) {
  const entries: { label: string; color?: string }[] = [
    { label: "计划中", color: statusColor.planned },
    { label: "进行中", color: statusColor.in_progress },
    { label: "已完成", color: statusColor.completed },
    { label: "依赖关系" },
  ];
  context.font = "12px -apple-system, BlinkMacSystemFont, sans-serif";
  context.textBaseline = "middle";
  let cursor = x;
  entries.forEach((entry, index) => {
    if (entry.color) {
      context.fillStyle = entry.color;
      context.beginPath();
      context.arc(cursor + 4, y, 4, 0, Math.PI * 2);
      context.fill();
      cursor += 14;
    } else {
      context.strokeStyle = "#6957e8";
      context.lineWidth = 2;
      context.beginPath();
      context.moveTo(cursor, y);
      context.lineTo(cursor + 13, y);
      context.stroke();
      context.fillStyle = "#6957e8";
      context.beginPath();
      context.moveTo(cursor + 13, y);
      context.lineTo(cursor + 8, y - 3);
      context.lineTo(cursor + 8, y + 3);
      context.closePath();
      context.fill();
      cursor += 20;
    }
    context.fillStyle = "#777489";
    context.fillText(entry.label, cursor, y);
    cursor += context.measureText(entry.label).width + (index < 3 ? 24 : 0);
  });
}

function drawNode(
  context: CanvasRenderingContext2D,
  task: Task,
  x: number,
  y: number,
  connected: boolean,
  accent: string,
  subtitle: string,
) {
  context.save();
  context.shadowColor = "rgba(41, 37, 68, 0.08)";
  context.shadowBlur = 14;
  context.shadowOffsetY = 5;
  context.fillStyle = "#ffffff";
  roundedRect(context, x, y, TASK_GRAPH_NODE_WIDTH, TASK_GRAPH_NODE_HEIGHT, 9);
  context.fill();
  context.restore();

  context.save();
  context.strokeStyle = connected ? "#dedce8" : "#c9c6d5";
  context.lineWidth = 1;
  context.setLineDash(connected ? [] : [5, 4]);
  roundedRect(context, x, y, TASK_GRAPH_NODE_WIDTH, TASK_GRAPH_NODE_HEIGHT, 9);
  context.stroke();
  context.restore();

  context.fillStyle = accent;
  roundedRect(context, x, y, TASK_GRAPH_NODE_WIDTH, 4, 4);
  context.fill();

  context.fillStyle = statusColor[task.status];
  context.beginPath();
  context.arc(x + 17, y + 23, 4, 0, Math.PI * 2);
  context.fill();

  context.textBaseline = "middle";
  context.font = "10px -apple-system, BlinkMacSystemFont, sans-serif";
  context.fillStyle = "#888596";
  context.fillText(
    fittedText(
      context,
      `${dayLabel(task.start)} · ${formatTime(task.start)}`,
      TASK_GRAPH_NODE_WIDTH - 38,
    ),
    x + 28,
    y + 23,
  );

  context.font = "700 14px -apple-system, BlinkMacSystemFont, sans-serif";
  context.fillStyle = "#302e40";
  context.fillText(
    fittedText(context, task.title, TASK_GRAPH_NODE_WIDTH - 28),
    x + 14,
    y + 50,
  );

  context.font = "10px -apple-system, BlinkMacSystemFont, sans-serif";
  context.fillStyle = "#858194";
  context.fillText(
    fittedText(context, subtitle, TASK_GRAPH_NODE_WIDTH - 28),
    x + 14,
    y + 73,
  );
  if (!connected) {
    context.font = "9px -apple-system, BlinkMacSystemFont, sans-serif";
    context.fillStyle = "#8c8799";
    context.fillText("未关联", x + 14, y + 92);
  }
}

export async function renderExperimentGraphPng(
  input: ExperimentGraphPngInput,
): Promise<Uint8Array> {
  const size = experimentGraphCanvasSize(input.graph);
  const canvas = document.createElement("canvas");
  canvas.width = size.pixelWidth;
  canvas.height = size.pixelHeight;
  const context = canvas.getContext("2d", { alpha: false });
  if (!context) throw new Error("当前系统无法创建 PNG 画布");
  context.scale(size.scale, size.scale);
  context.fillStyle = "#ffffff";
  context.fillRect(0, 0, size.logicalWidth, size.logicalHeight);

  context.fillStyle = "#6957e8";
  context.font = "800 11px -apple-system, BlinkMacSystemFont, sans-serif";
  context.fillText(input.experiment.code, PADDING, 33);
  context.fillStyle = "#242336";
  context.font = "700 26px -apple-system, BlinkMacSystemFont, sans-serif";
  context.fillText(input.experiment.title, PADDING, 65);
  context.fillStyle = "#77758a";
  context.font = "11px -apple-system, BlinkMacSystemFont, sans-serif";
  context.fillText(
    `${input.graph.nodes.length} Tasks${input.dateRange ? ` · ${input.dateRange}` : ""}`,
    PADDING,
    88,
  );
  drawLegend(context, PADDING, 108);

  context.save();
  context.translate(PADDING, HEADER_HEIGHT);
  context.fillStyle = "#fbfafe";
  context.fillRect(0, 0, input.graph.width, input.graph.height);
  context.fillStyle = "#ddd9e8";
  for (let y = 0; y < input.graph.height; y += 20)
    for (let x = 0; x < input.graph.width; x += 20) {
      context.beginPath();
      context.arc(x, y, 1, 0, Math.PI * 2);
      context.fill();
    }

  context.strokeStyle = "#9289d9";
  context.fillStyle = "#9289d9";
  context.lineWidth = 2;
  for (const edge of input.graph.edges) {
    context.stroke(new Path2D(edge.path));
    const child = input.graph.nodes.find(
      (node) => node.task.id === edge.childId,
    );
    if (!child) continue;
    const endY = child.y + TASK_GRAPH_NODE_HEIGHT / 2;
    context.beginPath();
    context.moveTo(child.x, endY);
    context.lineTo(child.x - 8, endY - 5);
    context.lineTo(child.x - 8, endY + 5);
    context.closePath();
    context.fill();
  }

  for (const node of input.graph.nodes)
    drawNode(
      context,
      node.task,
      node.x,
      node.y,
      node.connected,
      input.experiment.color,
      input.subtitleByTaskId[node.task.id] || "尚无 Record",
    );
  context.restore();

  context.fillStyle = "#918d9d";
  context.font = "10px -apple-system, BlinkMacSystemFont, sans-serif";
  context.textAlign = "right";
  context.fillText(
    "由 LabFlow 导出",
    size.logicalWidth - PADDING,
    size.logicalHeight - 12,
  );

  const blob = await new Promise<Blob>((resolve, reject) =>
    canvas.toBlob(
      (value) =>
        value ? resolve(value) : reject(new Error("PNG 编码失败，请重试")),
      "image/png",
    ),
  );
  canvas.width = 0;
  canvas.height = 0;
  return new Uint8Array(await blob.arrayBuffer());
}
