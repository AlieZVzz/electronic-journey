# Electronic Journey

[![CI](https://github.com/AlieZVzz/electronic-journey/actions/workflows/ci.yml/badge.svg)](https://github.com/AlieZVzz/electronic-journey/actions/workflows/ci.yml)
[![Latest release](https://img.shields.io/github/v/release/AlieZVzz/electronic-journey?include_prereleases&sort=semver)](https://github.com/AlieZVzz/electronic-journey/releases)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)

Electronic Journey 是一个 local-first 的桌面应用，用来记录个人的数字旅程。
在用户明确授权并主动开始记录后，它会按计划截取所选显示器，将原图和缩略图保存在本机，并提供本地时间线浏览、选择、删除和可选的个人 SFTP 上传。

<img width="1292" height="872" alt="image" src="https://github.com/user-attachments/assets/d68de9a9-f027-415f-94a8-b91a0c903545" />

<img width="1292" height="872" alt="Screenshot 2026-08-17 at 15 08 08" src="https://github.com/user-attachments/assets/bd979dd1-3fcf-4b5e-bd89-3097d15c2f21" />
<img width="1292" height="872" alt="image" src="https://github.com/user-attachments/assets/3790de6a-b741-494a-ab20-5583d46a2c21" />

> 项目目前处于早期开发阶段。macOS 原型和核心本地流程已经可以运行；Windows 捕获、系统事件和托盘行为仍需要在真实设备上完成验收。请不要把当前版本视为稳定生产版本。

## 为什么做这个项目

Electronic Journey 的设计目标是让记录过程可见、可控、可恢复：截图默认只留在设备上，应用不隐藏运行，不绕过系统权限，也不把图片发送到项目方的云端。用户可以在本地查看和管理时间线；如果需要备份，再明确选择图片并确认上传到自己管理的 SFTP 服务器。

## 特性

- **本地优先**：原图、缩略图和时间线元数据保存在本机 SQLite 与应用目录中。
- **明确授权**：依赖操作系统的屏幕录制权限；开始、暂停、停止和退出状态对用户可见。
- **可恢复的时间线**：使用数据库中的采集时间排序，启动时恢复索引，图片写入采用临时文件、同步和原子改名。
- **资源友好的存储**：保存无损 WebP 原图和有界 WebP 缩略图，并对重复画面进行稳定内容去重。
- **系统状态感知**：支持锁屏、休眠/唤醒和空闲状态下的安全暂停逻辑。
- **可选的个人服务器上传**：用户勾选图片并二次确认后，Rust 客户端才通过固定 SSH 主机指纹的 SFTP 上传原图。
- **窄权限边界**：前端不能直接访问任意文件系统、shell、截图或网络；敏感操作由 Rust Tauri 命令完成。

## 当前状态

| 能力 | 状态 |
| --- | --- |
| macOS 主显示器截图与屏幕录制权限流程 | 已实现，待 macOS 15+ 真机验收 |
| Windows.Graphics.Capture 适配器 | 已实现，待 Windows 11 真机验收 |
| 本地 WebP、缩略图、SQLite 时间线和启动恢复 | 已实现 |
| 锁屏、休眠/唤醒、空闲监听和托盘状态机 | 已实现，待两端真机验收 |
| 个人 SFTP 配置、手动选择上传、失败项重试/取消 | 已实现，部分平台凭据行为待验收 |
| 自动同步 | 已实现，默认关闭 |
| 稳定版发布和代码签名 | 尚未完成 |
| 用户主动检查与签名更新 | 已实现；操作系统代码签名尚未完成 |

详细进度见 `tasks/todo.md`，产品和安全约束见 `electronic-journey-design.md`。

## 支持平台

- macOS Sequoia 15 或更高版本
- Windows 11 或更高版本

当前没有 Linux 桌面支持计划。不同显示器、Retina/DPI、任务栏和权限撤销场景仍以真机验收结果为准。

## 快速开始

### 环境要求

- Node.js 20.19 或更高版本
- npm 10 或更高版本
- Rust stable（推荐通过 [rustup](https://rustup.rs/) 安装）
- Tauri 2 对应平台的系统依赖：
  - [macOS prerequisites](https://v2.tauri.app/start/prerequisites/#macos)
  - [Windows prerequisites](https://v2.tauri.app/start/prerequisites/#windows)

### 安装依赖

~~~bash
git clone https://github.com/AlieZVzz/electronic-journey.git
cd electronic-journey
scripts/init.sh
~~~

如果本机尚未安装 Rust，初始化脚本仍会安装前端依赖，并提示后续补装 Rust。

### 运行

在浏览器中运行安全的本地演示界面。该模式不会截图、读取文件、连接远程服务器或上传图片：

~~~bash
npm run dev
~~~

运行 Tauri 桌面应用：

~~~bash
npm run dev:desktop
~~~

调试 macOS 屏幕录制权限时，可以构建带稳定本地 TCC 身份的 debug 应用包：

~~~bash
npm run build:desktop:debug
~~~

如需清除该应用的旧屏幕录制授权记录：

~~~bash
tccutil reset ScreenCapture com.electronicjourney.app
~~~

## 开发与检查

常用命令：

~~~bash
npm run lint             # ESLint
npm run typecheck       # TypeScript 类型检查
npm test                # 前端测试
npm run build           # 前端生产构建
cargo check --workspace # Rust 编译检查
cargo test --workspace  # Rust 测试
scripts/check.sh        # 运行完整的本地检查
~~~

前端检查要求 Node.js 20.19.5；GitHub Actions 会在推送到 `main` 或创建 Pull Request 时运行前端检查，并在 macOS 与 Windows runner 上运行 Rust 检查和测试。

改动行为前请先阅读：

- `electronic-journey-design.md`：产品、安全边界和验收标准
- `docs/architecture.md`：系统架构与模块边界
- `docs/threat-model.md`：威胁模型
- `docs/api.md`：Tauri 命令边界
- `docs/file-format.md`：本地图片格式
- `docs/coding-style.md`：编码规范

## 隐私与安全边界

- 应用不隐蔽运行，不隐藏托盘图标，不绕过系统屏幕录制授权。
- 不记录键盘、剪贴板、浏览器历史、麦克风或窗口文本。
- 默认不上传图片，也不建设项目方的图片云、对象存储或 LLM 中转服务。
- 自动同步默认关闭；启用后只同步用户自管服务器上指定文件夹中的当天图片。
- 私钥口令只存入 macOS Keychain 或 Windows Credential Manager，不写入 SQLite、前端状态或日志。
- SFTP 连接会校验已保存的 SSH 主机 SHA-256 指纹；上传前后会校验文件完整性。
- 客户端不配置或感知 Hermes、提示词、模型和其他远端消费者，也不声称远端图片已被后续程序读取。

完整边界和数据流请参阅 `docs/threat-model.md` 与 `electronic-journey-design.md`。

## 发布桌面安装包

推送与应用版本一致的标签会触发 GitHub Actions 打包流程：

~~~bash
git tag v0.1.1
git push origin v0.1.1
~~~

工作流会构建：

- macOS Apple Silicon：DMG
- macOS Intel：DMG
- Windows x64：MSI 与 NSIS 安装程序

所有平台构建成功且预期安装包齐全后，Draft Release 才会自动公开。也可以从 GitHub Actions 的 **Package desktop apps** 页面手动触发构建；手动构建只生成 workflow artifacts，不会创建公开 Release。

发布前请注意：

- 应用版本必须同步更新 `package.json`、`package-lock.json`、`src-tauri/tauri.conf.json` 和根目录 `Cargo.toml`。
- 未配置 Apple 开发者证书时，macOS 使用 ad-hoc 签名，仅适合内部测试。
- Windows 安装包目前未进行 Authenticode 签名。
- 应用内更新包使用独立的 Tauri 更新签名验证来源和完整性；这不能替代 Apple Developer ID、公证或 Windows Authenticode。
- 当前发布允许用户主动检查 GitHub Release 并确认安装，但 macOS Gatekeeper 与 Windows SmartScreen 仍可能警告或阻止未做操作系统代码签名的安装包。
- 发布工作流需要仓库 Secret `TAURI_SIGNING_PRIVATE_KEY`。私钥不得写入仓库、日志或 Release；丢失后已安装客户端将无法信任后续更新。
- 本地 `tauri build` 也需要把 `TAURI_SIGNING_PRIVATE_KEY` 设置为私钥文件路径（或私钥内容）；无密码私钥同时设置空的 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`。debug app 脚本会显式关闭更新产物生成。

## 项目结构

~~~text
src/                    # React 表现层、页面、状态与 IPC 客户端
src-tauri/              # Tauri 壳、Rust 核心服务、平台适配器和 SQLite 迁移
docs/                   # 架构、安全、接口、文件格式与协作文档
scripts/                # 初始化、检查、版本同步与 debug 签名脚本
tasks/                  # 待办、进行中与完成记录
memory/                 # 项目决策和经验记录
electronic-journey-design.md
~~~

`server/` 中的代码属于早期控制面原型，不是当前产品的图片存储或同步路径。

## 参与贡献

欢迎提交 Issue、改进文档或 Pull Request。提交代码前请：

1. 先阅读产品设计、安全边界和相关模块文档。
2. 为新的业务逻辑补充聚焦测试，为 bug 修复补充回归测试。
3. 运行与改动相关的最小检查；条件允许时再运行 `scripts/check.sh`。
4. 在 Pull Request 中说明改动范围、验证命令、平台和已知风险。

安全问题请不要直接公开提交 Issue；请先通过仓库维护者提供的私密渠道联系。

## 路线图

近期工作包括完成 macOS/Windows 真机验收、补齐凭据和上传队列的崩溃恢复测试、完善缩略图失败后的后台重建，并决定正式发布渠道。更长期的方向包括本地 OCR/全文搜索、隐私遮挡、收藏标签和旅程包导出。

## 许可证

本项目基于 [MIT License](LICENSE) 发布。

## 致谢

本项目基于 [Tauri](https://tauri.app/)、[React](https://react.dev/)、[Vite](https://vite.dev/)、[Rust](https://www.rust-lang.org/)、[SQLite](https://www.sqlite.org/) 以及相关开源生态构建。
