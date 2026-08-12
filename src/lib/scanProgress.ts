import type { ScanProgress } from "../types";

const scanPhaseLabel: Record<ScanProgress["phase"], string> = {
  managers: "正在读取包管理器",
  projects: "正在查看已添加的项目目录",
  runtimes: "正在读取本机运行时",
  health: "正在生成健康摘要",
  complete: "扫描完成",
};

export function formatScanProgress(progress: ScanProgress): string {
  if (progress.phase === "complete") return scanPhaseLabel.complete;
  return `${scanPhaseLabel[progress.phase]} ${progress.completed}/${progress.total}`;
}
