# 云端 API

当前服务只实现 `GET /health` 和 `GET /v1/status`。以下接口是 Phase 2 契约，落地前需要认证、授权、限流、幂等和对象回查测试。

## 设备

```http
POST /v1/devices
POST /v1/devices/{device_id}/revoke
```

设备撤销后不能再申请新的上传或下载地址。

## 上传

```http
POST /v1/uploads/init
POST /v1/uploads/{upload_id}/complete
```

初始化请求只提交捕获 ID、密文大小、SHA-256、UTC 时间和加密元数据。返回的预签名地址必须：

- 只允许一个随机对象键和一个 HTTP 方法。
- 短时有效并限制内容大小。
- 不具备列目录、读取其他对象或删除对象的权限。
- 不进入日志、分析事件或错误报告。

完成接口必须从对象存储回查对象存在性、大小和摘要。回查成功前客户端不得将任务标记为 `completed`。

## 时间线和删除

```http
GET    /v1/captures?cursor=<cursor>&limit=50
DELETE /v1/captures/{capture_id}
```

时间线使用不透明游标。删除必须幂等；异步删除返回可查询状态，不得将数据库记录删除误报为对象也已删除。

## 通用要求

- 所有接口仅允许 HTTPS。
- 使用短期访问令牌和可撤销刷新令牌。
- 用户、设备和对象均执行资源级授权。
- 错误响应使用稳定脱敏错误码，不返回内部路径、对象 URL 或凭据。
- 上传初始化以用户 ID 和截图 ID 作为幂等键。
