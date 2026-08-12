import { useEffect, useState } from "react";
import { formatScanProgress } from "../lib/scanProgress";
import type { ScanProgress } from "../types";

const scanDescription = "扫描会读取本机包管理器、全局软件包、命令来源和运行时安装。不会修改任何文件。";

const outlinePoints = [
  "80 18 136 50 136 114 80 146 24 114 24 50 80 18",
  "80 12 112 64 112 100 80 150 48 100 48 64 80 12",
  "80 34 142 66 122 124 80 146 38 124 18 66 80 34",
  "80 18 136 50 136 114 80 146 24 114 24 50 80 18",
].join(";");

const foldPoints = ["24 50 80 82 136 50", "48 64 80 106 112 64", "18 66 80 46 142 66", "24 50 80 82 136 50"].join(";");

const spinePoints = ["80 82 80 146", "80 106 80 150", "80 46 80 146", "80 82 80 146"].join(";");

function usePrefersReducedMotion() {
  const [prefersReducedMotion, setPrefersReducedMotion] = useState(
    () => typeof window !== "undefined" && window.matchMedia?.("(prefers-reduced-motion: reduce)").matches,
  );

  useEffect(() => {
    if (typeof window === "undefined" || !window.matchMedia) return;
    const mediaQuery = window.matchMedia("(prefers-reduced-motion: reduce)");
    const updatePreference = () => setPrefersReducedMotion(mediaQuery.matches);
    mediaQuery.addEventListener("change", updatePreference);
    return () => mediaQuery.removeEventListener("change", updatePreference);
  }, []);

  return prefersReducedMotion;
}

interface ScanLauncherProps {
  error?: string;
  isInitializing: boolean;
  isScanning: boolean;
  progress?: ScanProgress;
  onCancel: () => void;
  onScan: () => void;
}

export function ScanLauncher({ error, isInitializing, isScanning, progress, onCancel, onScan }: ScanLauncherProps) {
  const prefersReducedMotion = usePrefersReducedMotion();
  const morphDuration = isScanning ? "2.8s" : "6.4s";
  const progressText = progress ? formatScanProgress(progress) : isScanning ? "正在准备扫描" : undefined;

  return (
    <section className={isScanning ? "scan-launcher scan-launcher--scanning" : "scan-launcher"} aria-label="环境扫描">
      <div className="scan-launcher__logo-stage" aria-hidden="true">
        <svg className="scan-launcher__wireframe" viewBox="0 0 160 160" fill="none" focusable="false">
          <title>动态线框 Logo</title>
          <polyline
            className="scan-launcher__wireframe-outline"
            points="80 18 136 50 136 114 80 146 24 114 24 50 80 18"
          >
            {!prefersReducedMotion ? (
              <animate
                key={`outline-${morphDuration}`}
                attributeName="points"
                calcMode="spline"
                dur={morphDuration}
                keySplines="0.65 0 0.35 1; 0.65 0 0.35 1; 0.65 0 0.35 1"
                keyTimes="0; 0.34; 0.68; 1"
                repeatCount="indefinite"
                values={outlinePoints}
              />
            ) : null}
          </polyline>
          <polyline className="scan-launcher__wireframe-fold" points="24 50 80 82 136 50">
            {!prefersReducedMotion ? (
              <animate
                key={`fold-${morphDuration}`}
                attributeName="points"
                calcMode="spline"
                dur={morphDuration}
                keySplines="0.65 0 0.35 1; 0.65 0 0.35 1; 0.65 0 0.35 1"
                keyTimes="0; 0.34; 0.68; 1"
                repeatCount="indefinite"
                values={foldPoints}
              />
            ) : null}
          </polyline>
          <polyline className="scan-launcher__wireframe-spine" points="80 82 80 146">
            {!prefersReducedMotion ? (
              <animate
                key={`spine-${morphDuration}`}
                attributeName="points"
                calcMode="spline"
                dur={morphDuration}
                keySplines="0.65 0 0.35 1; 0.65 0 0.35 1; 0.65 0 0.35 1"
                keyTimes="0; 0.34; 0.68; 1"
                repeatCount="indefinite"
                values={spinePoints}
              />
            ) : null}
          </polyline>
        </svg>
      </div>
      <button
        className="scan-launcher__button"
        type="button"
        disabled={isInitializing}
        onClick={isScanning ? onCancel : onScan}
      >
        {isScanning ? "取消扫描" : "扫描"}
      </button>
      <div className="scan-launcher__message" aria-live="polite">
        {progressText ? <span role="status">{progressText}</span> : scanDescription}
        {!isScanning && error ? (
          <>
            <br />
            <span role="alert">{error}</span>
          </>
        ) : null}
      </div>
    </section>
  );
}
