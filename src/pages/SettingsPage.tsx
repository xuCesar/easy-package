import { useState } from "react";
import { Icon } from "../components/Icon";
import { PageHeader } from "../components/PageHeader";
import type { ReportExportResult, ReportFormat, ScanSettings } from "../types";

interface SettingsPageProps {
  scanSettings: ScanSettings;
  onUpdateSettings: (settings: ScanSettings) => Promise<void>;
  onExportReport: (format: ReportFormat) => Promise<ReportExportResult>;
}

export function SettingsPage({ scanSettings, onUpdateSettings, onExportReport }: SettingsPageProps) {
  const [maxDepth, setMaxDepth] = useState(String(scanSettings.maxDepth));
  const [ignoredPath, setIgnoredPath] = useState("");
  const [reportFormat, setReportFormat] = useState<ReportFormat>("markdown");
  const [isSaving, setIsSaving] = useState(false);
  const [isExporting, setIsExporting] = useState(false);
  const [error, setError] = useState<string>();
  const [notice, setNotice] = useState<string>();

  const save = async (next: ScanSettings) => {
    setIsSaving(true);
    setError(undefined);
    setNotice(undefined);
    try {
      await onUpdateSettings(next);
      setMaxDepth(String(next.maxDepth));
      setNotice("系统扫描设置已更新。");
    } catch (saveError) {
      setError(saveError instanceof Error ? saveError.message : String(saveError));
    } finally {
      setIsSaving(false);
    }
  };

  const applyMaxDepth = async () => {
    const depth = Number(maxDepth);
    if (!Number.isInteger(depth) || depth < 1 || depth > 12) {
      setError("最大扫描深度需在 1 到 12 之间。");
      return;
    }
    await save({ ...scanSettings, maxDepth: depth });
  };

  const addIgnoredPath = async () => {
    const path = ignoredPath.trim();
    if (!path.startsWith("/")) {
      setError("请输入绝对目录路径。");
      return;
    }
    if (scanSettings.ignoredPaths.includes(path)) {
      setError("该目录已在忽略列表中。");
      return;
    }
    await save({ ...scanSettings, ignoredPaths: [...scanSettings.ignoredPaths, path] });
    setIgnoredPath("");
  };

  const exportReport = async () => {
    setIsExporting(true);
    setError(undefined);
    setNotice(undefined);
    try {
      const result = await onExportReport(reportFormat);
      setNotice(result.saved ? "环境报告已导出。" : "已取消导出。");
    } catch (exportError) {
      setError(exportError instanceof Error ? exportError.message : String(exportError));
    } finally {
      setIsExporting(false);
    }
  };

  return (
    <>
      <PageHeader
        title="系统设置"
        description="管理 Easy Package 的全局扫描策略与本地报告；不会开放任意命令执行能力。"
      />
      {error ? (
        <div className="inline-alert inline-alert--error">
          <Icon name="warning" />
          <span>{error}</span>
        </div>
      ) : null}
      {notice ? (
        <div className="inline-alert">
          <Icon name="info" />
          <span>{notice}</span>
        </div>
      ) : null}
      <section className="scan-settings panel" aria-label="系统扫描设置">
        <div className="panel__header">
          <h2>扫描与隐私</h2>
          <span className="quiet-label">应用于所有项目目录</span>
        </div>
        <div className="scan-settings__body">
          <div className="scan-setting">
            <div>
              <strong>最大扫描深度</strong>
              <p>控制项目目录遍历深度，范围 1–12。</p>
            </div>
            <div className="scan-setting__control">
              <label className="sr-only" htmlFor="max-depth">
                最大扫描深度
              </label>
              <input
                id="max-depth"
                type="number"
                min="1"
                max="12"
                value={maxDepth}
                onChange={(event) => setMaxDepth(event.target.value)}
              />
              <button className="button button--secondary" onClick={() => void applyMaxDepth()} disabled={isSaving}>
                应用
              </button>
            </div>
          </div>
          <div className="scan-setting">
            <div>
              <strong>联网策略</strong>
              <p>离线模式不会执行可能访问 registry 的更新检查。</p>
            </div>
            <div className="scan-setting__control">
              <label className="sr-only" htmlFor="network-policy">
                联网策略
              </label>
              <select
                id="network-policy"
                value={scanSettings.networkPolicy}
                onChange={(event) =>
                  void save({ ...scanSettings, networkPolicy: event.target.value as ScanSettings["networkPolicy"] })
                }
                disabled={isSaving}
              >
                <option value="offline">离线</option>
                <option value="registry">允许 registry 检查</option>
              </select>
            </div>
          </div>
          <div className="scan-setting">
            <div>
              <strong>默认忽略目录</strong>
              <p>构建产物与依赖缓存始终跳过。</p>
            </div>
            <section className="ignore-tags" aria-label="默认忽略目录">
              {scanSettings.defaultIgnoredDirectoryNames.map((name) => (
                <code key={name}>{name}</code>
              ))}
            </section>
          </div>
          <div className="scan-setting">
            <div>
              <strong>用户忽略目录</strong>
              <p>仅接受位于已添加扫描目录内的现有绝对路径。</p>
            </div>
            <div className="ignored-paths">
              {scanSettings.ignoredPaths.map((path) => (
                <span key={path}>
                  <code>{path}</code>
                  <button
                    className="icon-button icon-button--danger"
                    onClick={() =>
                      void save({
                        ...scanSettings,
                        ignoredPaths: scanSettings.ignoredPaths.filter((item) => item !== path),
                      })
                    }
                    aria-label={`移除忽略目录 ${path}`}
                    disabled={isSaving}
                  >
                    <Icon name="trash" />
                  </button>
                </span>
              ))}
              {scanSettings.ignoredPaths.length === 0 ? <small>尚未添加用户忽略目录。</small> : null}
              <div className="ignored-paths__add">
                <label className="sr-only" htmlFor="ignored-path">
                  忽略目录
                </label>
                <input
                  id="ignored-path"
                  value={ignoredPath}
                  onChange={(event) => setIgnoredPath(event.target.value)}
                  placeholder="/Users/me/Code/archive"
                />
                <button className="button button--secondary" onClick={() => void addIgnoredPath()} disabled={isSaving}>
                  添加忽略目录
                </button>
              </div>
            </div>
          </div>
          <div className="scan-setting">
            <div>
              <strong>环境报告</strong>
              <p>
                通过系统保存对话框导出；主目录会替换为 <code>~</code>。
              </p>
            </div>
            <div className="scan-setting__control">
              <label className="sr-only" htmlFor="report-format">
                报告格式
              </label>
              <select
                id="report-format"
                value={reportFormat}
                onChange={(event) => setReportFormat(event.target.value as ReportFormat)}
                disabled={isExporting}
              >
                <option value="markdown">Markdown</option>
                <option value="json">JSON</option>
              </select>
              <button className="button button--secondary" onClick={() => void exportReport()} disabled={isExporting}>
                {isExporting ? "导出中…" : "导出报告"}
              </button>
            </div>
          </div>
        </div>
      </section>
    </>
  );
}
