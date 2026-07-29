# 桌面客户端远程存储命令边界

客户端不调用 LLM，也不感知远端 Hermes。远程能力仅用于把用户明确选择并确认的本地原图上传到个人 SFTP 文件夹。

```text
get_remote_profile()
pick_private_key_file()
probe_remote_host_key(host, port)
save_remote_profile(input)
test_remote_profile()
upload_selected_captures(capture_ids)
get_upload_batch_status(batch_id)
get_active_upload_batch()
sync_today_now()
```

- `get_remote_profile` 返回脱敏配置和 `hasPassphrase`，不返回口令。
- `pick_private_key_file` 打开系统原生单文件选择器，并在 Rust 中验证所选私钥。
- `probe_remote_host_key` 只读取 SHA-256 指纹，用户仍需独立核对。
- `save_remote_profile` 只接受结构化字段；私钥口令进入系统钥匙串。
- `test_remote_profile` 创建、核对并删除零字节临时文件，不读取截图。
- `upload_selected_captures` 只接受 1 至 500 个截图 UUID；Rust 从 SQLite 获取受控本地路径和确定性远端路径，创建后台批次后立即返回，不等待 SFTP 完成。
- `get_upload_batch_status` 返回批次状态、实时成功/失败数量、逐项状态和脱敏错误。
- `get_active_upload_batch` 用于页面重新进入后恢复进度跟踪；同一时刻只允许一个 `pending` 或 `uploading` 批次。
- `sync_today_now` 仅在用户已显式启用自动同步且计划未因安全错误暂停时创建当天同步任务；截图范围完全由 Rust 从 SQLite 筛选。

前端不能传入图片本地路径、远端最终文件名、任意 shell 命令、私钥正文、Hermes 配置、提示词或模型参数。浏览器 fallback 对读取指纹、保存、测试和上传均必须明确失败，不能模拟成功。
