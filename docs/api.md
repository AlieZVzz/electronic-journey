# LLM 提供商调用边界

当前产品不建设图片云端 API、对象存储或服务端 LLM 中转。所有图片分析请求由桌面客户端的 Rust 核心直接调用用户配置的 LLM 提供商。

## 客户端内部命令

Tauri 命令只接受明确类型；LLM 命令仍属于后续能力：

```text
delete_timeline_capture(capture_id)
validate_llm_credential(provider)
create_ai_job(capture_ids, question, provider, model, confirmation)
cancel_ai_job(job_id)
get_ai_job(job_id)
```

- `capture_id` 只接受 UUID，不接受前端提供的路径。删除操作必须验证受控路径、
  SQLite 记录和文件均已移除后才返回成功；仍关联 AI 任务时拒绝删除。
- `capture_ids` 必须是有界 UUID 列表，不接受路径或 URL。
- `provider` 和 `model` 必须来自应用支持的枚举或审核后的目录。
- `confirmation` 必须绑定本次图片集合、提供商、模型和目标域名，不能复用于另一请求。
- React 不得获得 API Key、Authorization header 或任意 endpoint。

## 提供商适配器

每个适配器负责：

- 固定允许的 HTTPS 域名和端口。
- 凭据格式与最小验证请求。
- 图片数、单图大小、总字节数和 MIME 类型限制。
- 请求和响应的提供商格式映射。
- 连接、读取和总请求超时。
- 跨源重定向限制。
- 稳定且脱敏的错误码。

## 成功语义

- 凭据验证成功只表示当次最小请求通过，不表示图片请求一定成功。
- AI 任务只有在完整响应接收并解析后才能标记为 `completed`。
- 取消或超时不能被表述为提供商已经删除请求数据。
- 默认不自动重试包含图片的请求；用户手动重试前提示可能重复发送和计费。

## 日志禁令

不得记录图片、base64 请求体、API Key、令牌、Authorization header、用户问题全文、完整模型回答或提供商返回的敏感原始错误。
