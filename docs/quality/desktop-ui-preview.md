# 桌面 UI 本地预览

以下命令在用户打开的项目目录运行；这里的目标目录不代表需要创建 worktree。

- `npm run dev -w @fileterm/tauri` 必须加载 `tauri.dev.conf.json`：开发版显示为 `FileTerm Dev`，bundle ID 保持 `com.fileterm.desktop.dev`，但数据与日志解析到普通安装版的 `com.fileterm.desktop` 目录。旧 `.dev` 数据目录不迁移、不读取；Windows portable 继续使用 exe 旁的配置目录。
- 验证桌面 UI 时，从目标项目目录启动开发版，并核对启动日志、Vite 监听端口、bundle ID 和进程路径。Computer Use 通过 `com.fileterm.desktop.dev` 选择开发版，不得仅凭窗口标题识别版本。
- 如果默认 dev 端口 `5188` 已被目标项目目录的 Vite 服务占用，可复用该服务运行 `npm run dev -w @fileterm/tauri -- --config '{"build":{"beforeDevCommand":null}}'`；这仍会加载开发版身份配置。否则不要猜窗口版本、改用安装版或结束可能保有远程会话的进程。
- 只有确认窗口由当前项目目录的开发进程启动后，才用 Computer Use 操作或截图验证；无法区分开发版与安装版时，先报告具体阻碍。

## 云端无显示环境

Claude Code 云端会话由 `.claude/hooks/session-start.sh` 安装 WebKitGTK 构建依赖、Xvfb、截图工具和 OpenSSH（Rust 真实 sshd 测试用）。需要截图验证 Linux GUI 时运行 `.claude/scripts/linux-gui-preview.sh [输出.png] [等待秒数]`：它编译 debug 版（`tauri build --debug --no-bundle`），在 Xvfb `:99` 上以软件渲染启动并保存整屏截图。`FILETERM_SKIP_BUILD=1` 复用已编译的二进制，`FILETERM_KEEP_RUNNING=1` 让应用保持运行以便继续截图。
