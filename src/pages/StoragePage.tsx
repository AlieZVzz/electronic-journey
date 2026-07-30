import { type FormEvent, useEffect, useState } from "react";

import { desktopApi } from "../api/desktop";
import type { RemoteProfile, SaveRemoteProfileInput } from "../types/app";

const syncIntervals = [15, 30, 60, 120, 240] as const;

const emptyProfile: SaveRemoteProfileInput = {
  name: "个人服务器",
  host: "",
  port: 22,
  username: "",
  privateKeyPath: "",
  privateKeyPassphrase: null,
  hostKeyFingerprint: "",
  remoteRoot: "",
  autoSyncEnabled: false,
  syncIntervalMinutes: 30,
};

function formatDateTime(value: string | null): string {
  if (!value) {
    return "尚未安排";
  }
  return new Intl.DateTimeFormat("zh-CN", {
    month: "numeric",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(value));
}

function autoSyncStateText(profile: RemoteProfile): string {
  switch (profile.lastAutoSyncState) {
    case "running":
      return "正在同步当天未同步的原图";
    case "completed":
      return `上次同步完成：${profile.lastAutoSyncCompletedItems} 张成功`;
    case "partial_failed":
      return `上次同步：${profile.lastAutoSyncCompletedItems} 张成功，${profile.lastAutoSyncFailedItems} 张失败`;
    case "empty":
      return "上次检查时，当天没有待同步图片";
    case "skipped_busy":
      return "上次计划因已有上传任务而跳过";
    case "suspended":
      return "自动同步已暂停，需要重新验证配置";
    default:
      return "尚未执行自动同步";
  }
}

function suspendedReasonText(reason: string): string {
  const reasons: Record<string, string> = {
    invalid_profile: "服务器配置无效",
    invalid_key_path: "私钥路径已变化",
    invalid_key_file: "私钥文件无效或无法解锁",
    credential_store: "无法访问系统钥匙串",
    host_key_mismatch: "服务器主机指纹发生变化",
    authentication: "SSH 私钥认证失败",
  };
  return reasons[reason] ?? "配置需要重新验证";
}

export function StoragePage() {
  const [profile, setProfile] =
    useState<SaveRemoteProfileInput>(emptyProfile);
  const [hasSavedPassphrase, setHasSavedPassphrase] = useState(false);
  const [hasStoredProfile, setHasStoredProfile] = useState(false);
  const [remoteStatus, setRemoteStatus] = useState<RemoteProfile | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [probing, setProbing] = useState(false);
  const [testing, setTesting] = useState(false);
  const [pickingKey, setPickingKey] = useState(false);
  const [syncingNow, setSyncingNow] = useState(false);
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
          autoSyncEnabled: stored.autoSyncEnabled,
          syncIntervalMinutes: stored.syncIntervalMinutes,
        });
        setRemoteStatus(stored);
        setHasSavedPassphrase(stored.hasPassphrase);
        setHasStoredProfile(true);
      })
      .catch((reason) => active && setError(String(reason)))
      .finally(() => active && setLoading(false));
    return () => {
      active = false;
    };
  }, []);

  useEffect(() => {
    if (!hasStoredProfile) {
      return;
    }
    const timer = window.setInterval(() => {
      void desktopApi
        .getRemoteProfile()
        .then((stored) => {
          if (stored) {
            setRemoteStatus(stored);
            setHasSavedPassphrase(stored.hasPassphrase);
          }
        })
        .catch(() => {
          // Keep the last known status; explicit actions surface errors.
        });
    }, 15_000);
    return () => window.clearInterval(timer);
  }, [hasStoredProfile]);

  function update<K extends keyof SaveRemoteProfileInput>(
    key: K,
    value: SaveRemoteProfileInput[K],
  ) {
    setProfile((current) => ({ ...current, [key]: value }));
    setMessage(null);
    setError(null);
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
        setMessage("已选择私钥文件；保存配置后才会使用。");
      } else {
        setMessage("未选择文件，当前私钥路径没有改变。");
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
      setHasStoredProfile(true);
      setRemoteStatus(stored);
      setProfile((current) => ({
        ...current,
        autoSyncEnabled: stored.autoSyncEnabled,
        syncIntervalMinutes: stored.syncIntervalMinutes,
      }));
      setMessage(
        stored.autoSyncEnabled
          ? `配置已保存；自动同步已启用，每 ${stored.syncIntervalMinutes} 分钟检查当天图片。`
          : "配置已保存；自动同步保持关闭。",
      );
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
          : null,
      );
      if (!result.writable) {
        setError("远程目录不可写，请检查目录权限后重试。");
      } else {
        const stored = await desktopApi.getRemoteProfile();
        if (stored) {
          setRemoteStatus(stored);
        }
      }
    } catch (reason) {
      setError(String(reason));
    } finally {
      setTesting(false);
    }
  }

  async function syncNow() {
    setSyncingNow(true);
    setError(null);
    setMessage(null);
    try {
      await desktopApi.syncTodayNow();
      setMessage("已启动当天图片同步；任务会在后台继续执行。");
      const stored = await desktopApi.getRemoteProfile();
      if (stored) {
        setRemoteStatus(stored);
      }
    } catch (reason) {
      setError(String(reason));
    } finally {
      setSyncingNow(false);
    }
  }

  const operationBusy =
    loading || saving || probing || testing || pickingKey || syncingNow;

  return (
    <section className="storage-page">
      <header className="page-header">
        <div>
          <p className="eyebrow">PERSONAL SFTP STORAGE</p>
          <h1>远程存储</h1>
          <p>
            手动上传仍需逐项确认；自动同步只有在你明确开启后才会运行。
          </p>
        </div>
      </header>

      <div className="notice-card">
        <span aria-hidden="true">◆</span>
        <div>
          <strong>自动同步默认关闭</strong>
          <p>
            保存配置和“测试连接”本身不会上传截图。启用自动同步后，应用会按设定间隔上传当天尚未同步的原图。
          </p>
        </div>
      </div>

      <form
        aria-busy={operationBusy}
        className="settings-panel remote-settings"
        onSubmit={save}
      >
        <label>
          <span>配置名称</span>
          <input
            disabled={operationBusy}
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
            disabled={operationBusy}
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
            disabled={operationBusy}
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
            disabled={operationBusy}
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
              disabled={operationBusy}
              placeholder="尚未选择私钥文件"
              readOnly
              required
              value={profile.privateKeyPath}
            />
            <button
              className="button button--file-picker"
              aria-busy={pickingKey}
              disabled={operationBusy}
              onClick={() => void pickPrivateKey()}
              type="button"
            >
              {!pickingKey && (
                <svg aria-hidden="true" viewBox="0 0 20 20">
                  <path d="M2.75 5.75A1.75 1.75 0 0 1 4.5 4h3.05l1.5 1.75h6.45a1.75 1.75 0 0 1 1.75 1.75v6.75A1.75 1.75 0 0 1 15.5 16h-11a1.75 1.75 0 0 1-1.75-1.75v-8.5Z" />
                </svg>
              )}
              {pickingKey ? "正在选择…" : "选择文件"}
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
            disabled={operationBusy}
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
            disabled={operationBusy}
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
              disabled={operationBusy}
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
              aria-busy={probing}
              disabled={
                operationBusy || !profile.host.trim() || profile.port < 1
              }
              onClick={() => void probeFingerprint()}
              type="button"
            >
              {probing ? "读取中…" : "读取"}
            </button>
          </span>
        </label>
        <label className="toggle-row">
          <span>
            <strong>自动同步当天图片</strong>
            <small>默认关闭；开启后图片会定期离开本机</small>
          </span>
          <input
            checked={profile.autoSyncEnabled}
            disabled={operationBusy}
            onChange={(event) =>
              update("autoSyncEnabled", event.target.checked)
            }
            type="checkbox"
          />
        </label>
        <label>
          <span>
            自动同步间隔
            <small>从保存配置或上次同步开始计算</small>
          </span>
          <span className="select-control">
            <select
              disabled={operationBusy || !profile.autoSyncEnabled}
              onChange={(event) =>
                update("syncIntervalMinutes", Number(event.target.value))
              }
              value={profile.syncIntervalMinutes}
            >
              {syncIntervals.map((minutes) => (
                <option key={minutes} value={minutes}>
                  每 {minutes} 分钟
                </option>
              ))}
            </select>
          </span>
        </label>
        {profile.autoSyncEnabled && (
          <div className="auto-sync-disclosure" role="note">
            开启并保存后，应用会在后台连接已固定指纹的个人服务器，上传当天未同步的 WebP
            原图。暂停或停止截图不会关闭自动同步；你可以随时在这里关闭。
          </div>
        )}
        {remoteStatus?.autoSyncEnabled && (
          <div
            className={`auto-sync-status ${
              remoteStatus.autoSyncSuspendedReason ? "is-suspended" : ""
            }`}
          >
            <div>
              <strong>{autoSyncStateText(remoteStatus)}</strong>
              <span>
                {remoteStatus.autoSyncSuspendedReason
                  ? `暂停原因：${suspendedReasonText(
                      remoteStatus.autoSyncSuspendedReason,
                    )}`
                  : `下次检查：${formatDateTime(
                      remoteStatus.nextAutoSyncAtUtc,
                    )}`}
              </span>
            </div>
            <button
              className="button button--ghost"
              aria-busy={syncingNow}
              disabled={
                operationBusy ||
                Boolean(remoteStatus.autoSyncSuspendedReason) ||
                remoteStatus.lastAutoSyncState === "running"
              }
              onClick={() => void syncNow()}
              type="button"
            >
              {syncingNow ? "正在启动…" : "立即同步当天图片"}
            </button>
          </div>
        )}

        {loading && (
          <div className="form-progress remote-settings__status" role="status">
            正在读取已保存配置…
          </div>
        )}
        {error && (
          <div className="error-banner remote-settings__status" role="alert">
            {error}
          </div>
        )}
        {message && (
          <div
            className="success-banner remote-settings__status"
            role="status"
          >
            {message}
          </div>
        )}
        <div className="remote-settings__actions">
          <button
            className="button button--ghost"
            aria-busy={testing}
            disabled={operationBusy || !hasStoredProfile}
            onClick={() => void testConnection()}
            title={
              hasStoredProfile
                ? "验证已保存的配置"
                : "请先保存配置，再测试连接"
            }
            type="button"
          >
            {testing ? "正在验证…" : "测试已保存配置"}
          </button>
          <button
            className="button button--primary"
            aria-busy={saving}
            disabled={operationBusy}
            type="submit"
          >
            {saving ? "正在保存…" : "保存配置"}
          </button>
        </div>
      </form>
    </section>
  );
}
