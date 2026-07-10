import type { ManagedPackage, PackageManagerId, UpdateStatus } from "../types";

export const formatBytes = (value?: number): string => {
  if (value === undefined) return "—";
  if (value === 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const index = Math.min(Math.floor(Math.log(value) / Math.log(1024)), units.length - 1);
  return `${(value / 1024 ** index).toFixed(index > 2 ? 1 : 0)} ${units[index]}`;
};

export const formatRelativeTime = (value?: string): string => {
  if (!value) return "尚未扫描";
  const seconds = Math.max(0, Math.floor((Date.now() - new Date(value).getTime()) / 1000));
  if (seconds < 60) return "刚刚";
  if (seconds < 3600) return `${Math.floor(seconds / 60)} 分钟前`;
  if (seconds < 86_400) return `${Math.floor(seconds / 3600)} 小时前`;
  return new Intl.DateTimeFormat("zh-CN", { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" }).format(new Date(value));
};

export interface PackageFilters {
  query: string;
  manager: "all" | PackageManagerId;
  status: "all" | UpdateStatus;
}

export const filterPackages = (packages: ManagedPackage[], filters: PackageFilters): ManagedPackage[] => {
  const query = filters.query.trim().toLocaleLowerCase();
  return packages.filter((pkg) => {
    const matchesQuery = query.length === 0 || pkg.name.toLocaleLowerCase().includes(query);
    const matchesManager = filters.manager === "all" || pkg.managerId === filters.manager;
    const matchesStatus = filters.status === "all" || pkg.updateStatus === filters.status;
    return matchesQuery && matchesManager && matchesStatus;
  });
};

export const managerLabel: Record<PackageManagerId, string> = {
  homebrew: "Homebrew",
  npm: "npm",
  pnpm: "pnpm",
  uv: "uv",
  pip: "pip",
};
