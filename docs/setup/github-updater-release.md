# GitHub 公开发布与应用内更新

LabFlow macOS 0.1.7 起使用 Tauri v2 updater，从公开仓库 `echoechofu/labflow-releases` 的最新 GitHub Release 读取 `latest.json`。应用启动时静默检查；发现更高版本后先用系统原生窗口询问是否下载，下载完成并通过 updater 签名校验后，再询问是否安装并重启。本次只发布 macOS 0.1.7，Windows 暂时维持 0.1.5，且不包含应用内更新。

## 两种签名互不替代

- macOS `.app` 继续使用 ad-hoc 签名，不使用 Apple Developer Program、Developer ID 或 notarization。
- updater 的 `.app.tar.gz`、Windows NSIS 更新包必须使用 Tauri updater 私钥签名。该签名用于防止更新文件被替换，不能关闭，也不能消除 Gatekeeper 或 SmartScreen 提示。

公开密钥已写入 `eln-app/src-tauri/tauri.conf.json`。本机私钥保存在 `~/.tauri/labflow-updater.key`，不属于 Git 仓库。私钥丢失后，已经安装的客户端将无法验证用新密钥签名的后续更新，因此必须单独做加密备份，不能上传到 Release、提交到 Git 或发送给用户。

## GitHub Actions secrets

私有源码仓库需要配置两个 secrets：

- `LABFLOW_UPDATER_PRIVATE_KEY`：`~/.tauri/labflow-updater.key` 的完整内容；
- `LABFLOW_RELEASES_TOKEN`：可向 `echoechofu/labflow-releases` 创建 Release 并上传文件的 token。

可在已登录 GitHub CLI 的开发机执行：

```bash
gh secret set LABFLOW_UPDATER_PRIVATE_KEY \
  --repo echoechofu/labflow-eln \
  < ~/.tauri/labflow-updater.key

gh secret set LABFLOW_RELEASES_TOKEN \
  --repo echoechofu/labflow-eln
```

第二条命令会交互式读取 token。不要把 token 写进 shell 历史、工作流或源码。

## 发布流程

1. 同步修改 `eln-app/package.json`、`eln-app/src-tauri/Cargo.toml` 和 `eln-app/src-tauri/tauri.conf.json` 中的版本。
2. 将代码推送到私有源码仓库。
3. 在 Actions 中运行 **Build and publish macOS release**。
4. `release_tag` 必须等于 `v` 加应用版本，例如 `v0.1.7`。
5. 先保持 `publish=false`，下载并验收 Actions 产生的 macOS artifact。
6. 验收通过后，用相同 commit 和 tag 再运行一次并设置 `publish=true`。

本次工作流只原生构建 Apple Silicon macOS，验证 ad-hoc 签名，生成 updater 签名、SHA-256 文件和只包含 `darwin-aarch64` 的 `latest.json`，然后上传到公开下载仓库。以后发布带 updater 的 Windows 新版时，可为 manifest 生成脚本同时提供 Windows 更新包及签名；本次不会生成或上传 Windows 0.1.7。

## 首次启用和测试

macOS 0.1.6 及更早版本没有 updater，无法自行发现 0.1.7。现有用户需要手动安装一次 macOS 0.1.7；从 0.1.7 升级到更高版本时才会出现应用内提醒。

macOS 端到端测试必须发布一个版本号更高的测试 Release，例如在安装 0.1.7 后发布 0.1.8。检查以下过程：

1. 启动 0.1.7 后出现系统更新确认窗口；
2. 拒绝时不下载，下次启动仍会提醒；
3. 接受后显示下载进度；
4. 下载完成后出现安装与重启确认；
5. 重启后应用版本已更新，本地 SQLite 和附件仍存在。

ad-hoc 签名不会获得 Apple 的开发者身份信任。首次安装、更新后系统重新评估应用，或 macOS 安全策略变化时，用户仍可能需要在“系统设置 → 隐私与安全性”中手动允许打开。该发布方式不能保证完全没有 Gatekeeper 提示。
