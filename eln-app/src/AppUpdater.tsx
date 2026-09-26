import { useEffect, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { ask, message } from "@tauri-apps/plugin-dialog";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";

type UpdatePhase = "downloading" | "ready" | "installing" | "error";

interface UpdateViewState {
  phase: UpdatePhase;
  version: string;
  downloadedBytes?: number;
  totalBytes?: number;
  detail?: string;
}

let currentState: UpdateViewState | undefined;
let startupCheck: Promise<void> | undefined;
const listeners = new Set<(state: UpdateViewState | undefined) => void>();

const publish = (state: UpdateViewState | undefined) => {
  currentState = state;
  for (const listener of listeners) listener(state);
};

const releaseNotes = (update: Update) => {
  const body = update.body?.trim();
  if (!body) return "此版本包含新的功能与修复。";
  return body.length > 1200 ? `${body.slice(0, 1200)}…` : body;
};

const runStartupCheck = async () => {
  let update: Update | null = null;
  let userAccepted = false;
  try {
    update = await check({ timeout: 30_000 });
    if (!update) return;

    userAccepted = await ask(
      `发现 LabFlow ${update.version}（当前版本 ${update.currentVersion}）。\n\n${releaseNotes(update)}\n\n是否下载更新？`,
      {
        title: "LabFlow 软件更新",
        kind: "info",
        okLabel: "下载更新",
        cancelLabel: "稍后提醒",
      },
    );
    if (!userAccepted) return;

    let downloadedBytes = 0;
    let totalBytes: number | undefined;
    publish({ phase: "downloading", version: update.version });
    await update.download(
      (event) => {
        if (event.event === "Started") {
          totalBytes = event.data.contentLength;
        } else if (event.event === "Progress") {
          downloadedBytes += event.data.chunkLength;
        }
        publish({
          phase: event.event === "Finished" ? "ready" : "downloading",
          version: update!.version,
          downloadedBytes,
          totalBytes,
        });
      },
      { timeout: 10 * 60_000 },
    );

    const installNow = await ask(
      `LabFlow ${update.version} 已下载并通过签名校验。\n\n安装时应用会关闭并重新启动，请先保存正在编辑的内容。`,
      {
        title: "安装并重启 LabFlow",
        kind: "info",
        okLabel: "安装并重启",
        cancelLabel: "暂不安装",
      },
    );
    if (!installNow) {
      publish(undefined);
      return;
    }

    publish({ phase: "installing", version: update.version });
    await update.install({ restartAfterInstall: true });
    await relaunch();
  } catch (cause) {
    const detail = cause instanceof Error ? cause.message : String(cause);
    if (!userAccepted) {
      console.info("LabFlow update check unavailable:", detail);
      return;
    }
    publish({
      phase: "error",
      version: update?.version || "",
      detail,
    });
    await message(`更新未完成：${detail}`, {
      title: "LabFlow 软件更新",
      kind: "error",
      okLabel: "知道了",
    });
  } finally {
    await update?.close().catch(() => undefined);
  }
};

const subscribe = (listener: (state: UpdateViewState | undefined) => void) => {
  listeners.add(listener);
  listener(currentState);
  return () => {
    listeners.delete(listener);
  };
};

const startStartupCheck = () => {
  if (!isTauri()) return;
  startupCheck ||= runStartupCheck();
};

const formatBytes = (value: number) => {
  if (value < 1024 * 1024) return `${Math.round(value / 1024)} KB`;
  return `${(value / 1024 / 1024).toFixed(1)} MB`;
};

export function AppUpdater() {
  const [state, setState] = useState<UpdateViewState>();
  useEffect(() => {
    const unsubscribe = subscribe(setState);
    startStartupCheck();
    return unsubscribe;
  }, []);

  if (!state) return null;
  const percentage = state.totalBytes
    ? Math.min(
        100,
        Math.round(((state.downloadedBytes || 0) / state.totalBytes) * 100),
      )
    : undefined;
  const status =
    state.phase === "downloading"
      ? `正在下载 LabFlow ${state.version}`
      : state.phase === "ready"
        ? "更新已下载，等待确认安装"
        : state.phase === "installing"
          ? "正在安装更新并准备重启"
          : "更新未完成";

  return (
    <div className="overlay centered app-update-overlay">
      <section
        className="modal app-update-window"
        role="status"
        aria-live="polite"
      >
        <p className="eyebrow">LABFLOW UPDATE</p>
        <h2>{status}</h2>
        {state.phase === "downloading" && (
          <>
            <div className="app-update-progress">
              <i style={{ width: `${percentage ?? 8}%` }} />
            </div>
            <p>
              {formatBytes(state.downloadedBytes || 0)}
              {state.totalBytes ? ` / ${formatBytes(state.totalBytes)}` : ""}
              {percentage !== undefined ? ` · ${percentage}%` : ""}
            </p>
          </>
        )}
        {state.phase === "installing" && (
          <p>请保持 LabFlow 运行，完成后会自动重新打开。</p>
        )}
        {state.phase === "error" && (
          <>
            <p className="form-error">{state.detail}</p>
            <button className="secondary" onClick={() => publish(undefined)}>
              关闭
            </button>
          </>
        )}
      </section>
    </div>
  );
}
