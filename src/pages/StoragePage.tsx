export function StoragePage() {
  return (
    <section className="placeholder-page">
      <h1>存储与安全</h1>
      <p>管理本地保险箱、恢复密钥、云同步和已授权设备。</p>
      <div className="notice-card">
        <span aria-hidden="true">◆</span>
        <div>
          <strong>当前为仅本地模式</strong>
          <p>
            云同步将在第二阶段接入。初始化版本不会上传任何截图或敏感元数据。
          </p>
        </div>
      </div>
    </section>
  );
}
