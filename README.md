# Electronic Journey

Electronic Journey 是一款面向个人用户的数字旅程记录工具。它将在用户明确授权并主动开启后，按计划截取所选显示器，并在离开本机前完成认证加密。

当前仓库处于 `0.1.0` 初始化阶段：React 界面、Tauri IPC、Rust 核心模块边界、SQLite/PostgreSQL 初始迁移和 Axum 健康检查已经建立；真实平台截图、安全密钥存储、托盘与云同步尚未实现。

## 技术栈

- Tauri 2 + Rust
- React 19 + TypeScript + Vite 6
- SQLite + `sqlx`
- XChaCha20-Poly1305
- Axum + PostgreSQL

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

浏览器中运行界面（使用安全的本地演示状态，不执行截图或上传）：

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
open "target/debug/bundle/macos/Electronic Journey.app"
```

切换到稳定 debug 身份后如需清除旧授权记录，可执行一次：

```bash
tccutil reset ScreenCapture com.electronicjourney.app
```

运行控制面 API：

```bash
npm run server:dev
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
├── server/                 # Axum 控制面 API 与 PostgreSQL 迁移
├── docs/                   # 长期架构、安全、接口与协作文档
├── tasks/                  # 待办、进行中与完成记录
├── memory/                 # 长期背景、决策和经验
├── scripts/                # 初始化、检查与锁文件同步入口
├── .codex/                 # 项目级 Codex 配置和命令规则
└── electronic-journey-design.md
```

## 隐私边界

- 不隐蔽运行，不隐藏托盘图标，不绕过系统截图授权。
- 默认只保存加密的 `.ejourney` 文件。
- 不记录键盘、剪贴板、浏览器历史、麦克风或窗口文本。
- 云端不持有明文主密钥，不生成明文缩略图，不分析截图内容。
- 当前初始化版本不会执行真实截图，也不会上传任何数据。

完整设计、阶段和验收标准见
[`electronic-journey-design.md`](electronic-journey-design.md)。

## License

许可证尚未确定。项目发布前需完成许可证决策。
