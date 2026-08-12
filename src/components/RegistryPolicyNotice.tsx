import { useState } from "react";
import { apiErrorMessage } from "../lib/apiError";
import type { ScanSettings } from "../types";
import { Icon } from "./Icon";

interface RegistryPolicyNoticeProps {
  scanSettings: ScanSettings;
  onUpdateSettings: (settings: ScanSettings) => Promise<void>;
  onRefresh: () => void;
}

export function RegistryPolicyNotice({ scanSettings, onUpdateSettings, onRefresh }: RegistryPolicyNoticeProps) {
  const [status, setStatus] = useState<"idle" | "saving" | "enabled">("idle");
  const [error, setError] = useState<string>();

  const allowRegistryChecks = async () => {
    setStatus("saving");
    setError(undefined);
    try {
      await onUpdateSettings({ ...scanSettings, networkPolicy: "registry" });
      setStatus("enabled");
    } catch (updateError) {
      setStatus("idle");
      setError(apiErrorMessage(updateError, "联网策略更新失败，请重试。"));
    }
  };

  if (status === "enabled") {
    return (
      <div className="inline-alert" role="status">
        <Icon name="info" />
        <span>已允许 registry 检查。需要重新扫描后才会显示可更新状态。</span>
        <button
          onClick={() => {
            setStatus("idle");
            onRefresh();
          }}
        >
          重新扫描
        </button>
      </div>
    );
  }

  if (scanSettings.networkPolicy === "registry") return null;

  return (
    <div className={error ? "inline-alert inline-alert--error" : "inline-alert"} role={error ? "alert" : "status"}>
      <Icon name={error ? "warning" : "info"} />
      <span>{error ?? "当前为离线模式，不会查询软件包最新版本或搜索软件包目录。"}</span>
      <button onClick={() => void allowRegistryChecks()} disabled={status === "saving"}>
        {status === "saving" ? "正在允许…" : "允许检查更新"}
      </button>
    </div>
  );
}
