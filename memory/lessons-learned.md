# 踩坑与经验

## 2026-07-28 - 初始化环境缺少 Rust 工具链

- 现象：本机可用 Node.js 和 npm，但没有 `rustc` 与 `cargo`。
- 原因：工作区尚未配置 Tauri/Rust 开发环境。
- 解决：项目初始化脚本允许先完成前端安装，并明确提示通过 rustup 安装 Rust 后运行完整检查。
- 以后注意：没有运行 `cargo check` 和真机 Tauri 启动前，不得声称桌面端已通过编译。

## 2026-07-28 - Notion 示例配置需要按当前文档校验

- 现象：参考模板包含项目级 `model_provider` 和空的 `include_only`。
- 原因：模板用于说明结构，部分字段语义会随 Codex 版本演进。
- 解决：项目配置省略项目层无效的 provider，并省略会形成空白名单的 `include_only`。
- 以后注意：项目级 Codex 配置和 rules 变更应先对照当前官方配置参考。
