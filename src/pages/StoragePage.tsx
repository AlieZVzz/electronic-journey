export function StoragePage() {
  return (
    <section className="placeholder-page">
      <h1>存储与 AI</h1>
      <p>管理本地图片、保留策略和未来的 LLM 提供商凭据。</p>
      <div className="notice-card">
        <span aria-hidden="true">◆</span>
        <div>
          <strong>图片只保存在本机</strong>
          <p>
            当前不会自动上传任何截图。客户端直连 LLM 功能接入后，也只会发送你明确选择并确认的图片。
          </p>
        </div>
      </div>
    </section>
  );
}
