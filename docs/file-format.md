# `.ejourney` 文件格式

当前文件只定义容器契约；完整解析器和主密钥包装尚未实现。

## 二进制布局

```text
magic                 4 bytes   "EJRN"
format_version        2 bytes
algorithm_id          2 bytes
record_id            16 bytes
key_version           4 bytes
wrapped_key_length    4 bytes
wrapped_data_key      variable
nonce                24 bytes
ciphertext            variable
authentication_tag   16 bytes
```

## 规则

- 多字节整数编码方式必须在实现前固定，建议网络字节序。
- `wrapped_key_length` 和总文件大小必须有严格上限。
- 解析器在分配内存前验证所有长度和剩余字节数。
- `magic`、格式版本、记录 ID 和密钥版本进入认证附加数据。
- 未知算法、未知必需字段或不支持的版本必须明确失败。
- 认证标签验证完成前不得输出任何明文。
- 解析器不得跟随容器内容指定的路径或 URL。

## 版本策略

- 向后兼容读取已发布版本。
- 写入只使用当前版本。
- 格式迁移先生成新文件、验证可读，再原子替换。
- 不在原文件上原地修改密文。
