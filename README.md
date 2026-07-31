# Electronic Journey

Electronic Journey 是一款面向个人用户的数字旅程记录工具。它将在用户明确授权并主动开启后，按计划截取所选显示器，在本机保存原图和缩略图，并支持由用户明确选择和确认后上传到个人 SFTP 文件夹。

当前 macOS 原型已接通屏幕录制权限和主显示器截图，Windows.Graphics.Capture 显示器截图适配器也已加入。两端共用普通 WebP 与缩略图原子写入、SQLite 时间线和启动索引恢复；macOS 与 Windows 的锁屏、休眠/唤醒和空闲监听以及动态托盘控制已接入同一 Rust 状态机。个人 SFTP 配置、上传队列与手动选择界面已进入第一期实现，Windows 捕获、系统事件与托盘真机验收仍待完成。

最低运行版本为 macOS Sequoia 15 和 Windows 11。

## 技术栈

- Tauri 2 + Rust
- React 19 + TypeScript + Vite 6
- SQLite + `sqlx`
- macOS Keychain / Windows Credential Manager（用于私钥口令）
- Rust `russh` 与 `russh-sftp`（用于固定主机指纹的个人服务器上传）

## 环境要求

- Node.js 20.19 或更高版本
- npm 10 或更高版本
- Rust stable（通过 [rustup](https://rustup.rs/) 安装）
- Tauri 对应平台的系统依赖：
  - [macOS prerequisites](https://v2.tauri.app/start/prerequisites/#macos)
  - [Windows prerequisites](https://v2.tauri.app/start/prerequisites/#windows)

首次安装：

```bash
scripts/init.sh
```

本机尚未安装 Rust 时，脚本仍会安装前端依赖，并提示补装 Rust 后继续。

## 本地开发

浏览器中运行界面（使用安全的本地演示状态，不执行截图、文件读取、远程连接或上传）：

```bash
npm run dev
```

运行桌面应用：

```bash
npm run dev:desktop
```

调试 macOS 屏幕录制权限时，使用带稳定本地 TCC 身份的 debug
应用包，避免普通 `tauri dev` 重编译后被系统识别成另一个可执行文件：

```bash
npm run build:desktop:debug
```

构建与本地签名验证成功后，命令会自动打开 debug 应用。

切换到稳定 debug 身份后如需清除旧授权记录，可执行一次：

```bash
tccutil reset ScreenCapture com.electronicjourney.app
```

运行检查：

```bash
scripts/check.sh
```

## 项目结构

```text
electronic-journey/
├── src/                    # React 表现层、页面、状态与 IPC 客户端
├── src-tauri/              # Tauri 桌面壳、Rust 核心服务与 SQLite 迁移
│   ├── capabilities/       # 最小权限能力声明
│   ├── migrations/
│   └── src/
├── server/                 # 早期云同步原型；当前产品路径不使用
├── docs/                   # 长期架构、安全、接口与协作文档
├── tasks/                  # 待办、进行中与完成记录
├── memory/                 # 长期背景、决策和经验
├── scripts/                # 初始化、检查与锁文件同步入口
├── .codex/                 # 项目级 Codex 配置和命令规则
└── electronic-journey-design.md
```

## 隐私边界

- 不隐蔽运行，不隐藏托盘图标，不绕过系统截图授权。
- 新截图默认只在本机保存普通 WebP 原图和缩略图。
- 不记录键盘、剪贴板、浏览器历史、麦克风或窗口文本。
- 不建设自有图片云端、对象存储或 LLM 图片中转服务。
- 只有在用户选择图片并确认后，Rust 客户端才会通过 SFTP 上传到已配置的个人服务器文件夹。
- 客户端不配置或感知远端 Hermes、提示词、模型和其他图片消费者。
- 自动同步默认关闭；只有用户在远程存储中显式开启后，才会按设定间隔同步当天未同步图片。

完整设计、阶段和验收标准见
[`electronic-journey-design.md`](electronic-journey-design.md)。

## License

许可证尚未确定。项目发布前需完成许可证决策。
