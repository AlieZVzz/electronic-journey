import { type FormEvent, useEffect, useState } from "react";

import { desktopApi } from "../api/desktop";
import type { SaveRemoteProfileInput } from "../types/app";

const emptyProfile: SaveRemoteProfileInput = {
  name: "个人服务器",
  host: "",
  port: 22,
  username: "",
  privateKeyPath: "",
  privateKeyPassphrase: null,
  hostKeyFingerprint: "",
  remoteRoot: "",
};

export function StoragePage() {
  const [profile, setProfile] =
    useState<SaveRemoteProfileInput>(emptyProfile);
  const [hasSavedPassphrase, setHasSavedPassphrase] = useState(false);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [probing, setProbing] = useState(false);
  const [testing, setTesting] = useState(false);
  const [pickingKey, setPickingKey] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    void desktopApi
      .getRemoteProfile()
      .then((stored) => {
        if (!active || !stored) {
          return;
        }
        setProfile({
          name: stored.name,
          host: stored.host,
          port: stored.port,
          username: stored.username,
          privateKeyPath: stored.privateKeyPath,
          privateKeyPassphrase: null,
          hostKeyFingerprint: stored.hostKeyFingerprint,
          remoteRoot: stored.remoteRoot,
        });
        setHasSavedPassphrase(stored.hasPassphrase);
      })
      .catch((reason) => active && setError(String(reason)))
      .finally(() => active && setLoading(false));
    return () => {
      active = false;
    };
  }, []);

  function update<K extends keyof SaveRemoteProfileInput>(
    key: K,
    value: SaveRemoteProfileInput[K],
  ) {
    setProfile((current) => ({ ...current, [key]: value }));
    setMessage(null);
  }

  async function probeFingerprint() {
    setProbing(true);
    setError(null);
    setMessage(null);
    try {
      const fingerprint = await desktopApi.probeRemoteHostKey(
        profile.host.trim(),
        profile.port,
      );
      update("hostKeyFingerprint", fingerprint);
      setMessage(
        "已读取服务器指纹。请通过服务器控制台或管理员提供的信息独立核对后再保存。",
      );
    } catch (reason) {
      setError(String(reason));
    } finally {
      setProbing(false);
    }
  }

  async function pickPrivateKey() {
    setPickingKey(true);
    setError(null);
    setMessage(null);
    try {
      const selected = await desktopApi.pickPrivateKeyFile();
      if (selected) {
        update("privateKeyPath", selected);
      }
    } catch (reason) {
      setError(String(reason));
    } finally {
      setPickingKey(false);
    }
  }

  async function save(event: FormEvent) {
    event.preventDefault();
    setSaving(true);
    setError(null);
    setMessage(null);
    try {
      const stored = await desktopApi.saveRemoteProfile({
        ...profile,
        privateKeyPassphrase:
          profile.privateKeyPassphrase?.length === 0
            ? null
            : profile.privateKeyPassphrase,
      });
      setProfile((current) => ({
        ...current,
        privateKeyPassphrase: null,
      }));
      setHasSavedPassphrase(stored.hasPassphrase);
      setMessage("配置已保存；私钥口令（如有）只保存在系统钥匙串中。");
    } catch (reason) {
      setError(String(reason));
    } finally {
      setSaving(false);
    }
  }

  async function testConnection() {
    setTesting(true);
    setError(null);
    setMessage(null);
    try {
      const result = await desktopApi.testRemoteProfile();
      setMessage(
        result.writable
          ? `连接、主机指纹、私钥认证和目录写入验证均通过：${result.remoteRoot}`
          : "远程目录不可写。",
      );
    } catch (reason) {
      setError(String(reason));
    } finally {
      setTesting(false);
    }
  }

  return (
    <section className="storage-page">
      <header className="page-header">
        <div>
          <p className="eyebrow">PERSONAL SFTP STORAGE</p>
          <h1>远程存储</h1>
          <p>
            客户端只上传你在时间线中明确勾选并确认的原图，不感知远端的分析程序。
          </p>
        </div>
      </header>

      <div className="notice-card">
        <span aria-hidden="true">◆</span>
        <div>
          <strong>不会自动上传</strong>
          <p>
            保存配置和“测试连接”不会上传截图。测试会在目标目录创建并删除一个空的临时文件，以验证写权限。
          </p>
        </div>
      </div>

      <form className="settings-panel remote-settings" onSubmit={save}>
        <label>
          <span>配置名称</span>
          <input
            disabled={loading}
            maxLength={64}
            onChange={(event) => update("name", event.target.value)}
            required
            value={profile.name}
          />
        </label>
        <label>
          <span>服务器地址</span>
          <input
            autoCapitalize="none"
            disabled={loading}
            onChange={(event) => update("host", event.target.value)}
            placeholder="server.example.com"
            required
            spellCheck={false}
            value={profile.host}
          />
        </label>
        <label>
          <span>SSH 端口</span>
          <input
            disabled={loading}
            max={65535}
            min={1}
            onChange={(event) =>
              update("port", Number(event.target.value))
            }
            required
            type="number"
            value={profile.port}
          />
        </label>
        <label>
          <span>用户名</span>
          <input
            autoCapitalize="none"
            disabled={loading}
            onChange={(event) => update("username", event.target.value)}
            required
            spellCheck={false}
            value={profile.username}
          />
        </label>
        <label>
          <span>
            私钥路径
            <small>选择 PEM 或 OpenSSH 私钥；其他用户不能拥有读取权限</small>
          </span>
          <span className="remote-settings__file">
            <input
              aria-label="已选择的私钥文件"
              disabled={loading}
              placeholder="尚未选择私钥文件"
              readOnly
              required
              value={profile.privateKeyPath}
            />
            <button
              className="button button--ghost"
              disabled={loading || pickingKey}
              onClick={() => void pickPrivateKey()}
              type="button"
            >
              {pickingKey ? "正在选择…" : "选择文件…"}
            </button>
          </span>
        </label>
        <label>
          <span>
            私钥口令
            <small>
              {hasSavedPassphrase
                ? "已保存在系统钥匙串；留空会继续使用"
                : "无口令时留空"}
            </small>
          </span>
          <input
            autoComplete="current-password"
            disabled={loading}
            onChange={(event) =>
              update("privateKeyPassphrase", event.target.value)
            }
            type="password"
            value={profile.privateKeyPassphrase ?? ""}
          />
        </label>
        <label>
          <span>
            远程文件夹
            <small>必须是已经存在的绝对路径</small>
          </span>
          <input
            autoCapitalize="none"
            disabled={loading}
            onChange={(event) => update("remoteRoot", event.target.value)}
            placeholder="/srv/electronic-journey/inbox"
            required
            spellCheck={false}
            value={profile.remoteRoot}
          />
        </label>
        <label>
          <span>
            SSH 主机指纹
            <small>保存前应从独立渠道核对 SHA-256 指纹</small>
          </span>
          <span className="remote-settings__fingerprint">
            <input
              disabled={loading}
              onChange={(event) =>
                update("hostKeyFingerprint", event.target.value)
              }
              placeholder="SHA256:…"
              required
              spellCheck={false}
              value={profile.hostKeyFingerprint}
            />
            <button
              className="button button--ghost"
              disabled={probing || !profile.host || profile.port < 1}
              onClick={() => void probeFingerprint()}
              type="button"
            >
              {probing ? "读取中…" : "读取"}
            </button>
          </span>
        </label>

        {error && (
          <div className="error-banner remote-settings__status" role="alert">
            {error}
          </div>
        )}
        {message && (
          <div className="success-banner remote-settings__status">
            {message}
          </div>
        )}
        <div className="remote-settings__actions">
          <button
            className="button button--ghost"
            disabled={testing || saving || loading}
            onClick={() => void testConnection()}
            type="button"
          >
            {testing ? "正在验证…" : "测试已保存配置"}
          </button>
          <button
            className="button button--primary"
            disabled={saving || loading}
            type="submit"
          >
            {saving ? "正在保存…" : "保存配置"}
          </button>
        </div>
      </form>
    </section>
  );
}
