import type { ReactNode } from "react";
import type { PageId } from "../types";
import { Icon, type IconName } from "./Icon";

const navItems: Array<{ id: PageId; label: string; icon: IconName }> = [
  { id: "overview", label: "概览", icon: "overview" },
  { id: "packages", label: "软件包", icon: "packages" },
  { id: "actions", label: "操作", icon: "terminal" },
  { id: "projects", label: "项目", icon: "projects" },
  { id: "environment", label: "诊断", icon: "environment" },
];

const diagnosticPages = new Set<PageId>(["environment", "analysis", "runtimes", "history", "logs"]);

interface AppShellProps {
  page: PageId;
  onNavigate: (page: PageId) => void;
  children: ReactNode;
}

export function AppShell({ page, onNavigate, children }: AppShellProps) {
  return (
    <div className="app-shell">
      <header className="app-topbar" data-tauri-drag-region>
        <div className="app-topbar__primary" data-tauri-drag-region>
          <button className="brand" onClick={() => onNavigate("overview")} aria-label="返回概览" title="返回概览">
            <span className="brand__mark">
              <Icon name="packages" />
            </span>
          </button>
          <nav className="nav" aria-label="主要导航" data-tauri-drag-region>
            {navItems.map((item) => (
              <button
                key={item.id}
                className={`nav__item ${(item.id === "environment" ? diagnosticPages.has(page) : page === item.id) ? "nav__item--active" : ""}`}
                onClick={() => onNavigate(item.id)}
                aria-current={
                  (item.id === "environment" ? diagnosticPages.has(page) : page === item.id) ? "page" : undefined
                }
              >
                <Icon name={item.icon} className="nav__icon" />
                <span>{item.label}</span>
              </button>
            ))}
          </nav>
        </div>
        <div className="app-topbar__actions">
          <button className="safe-mode-button" onClick={() => onNavigate("actions")} title="打开操作中心">
            <span className="safe-mode-button__dot" />
            <span>安全操作模式</span>
          </button>
          <button
            className={page === "settings" ? "topbar-settings topbar-settings--active" : "topbar-settings"}
            onClick={() => onNavigate("settings")}
            aria-current={page === "settings" ? "page" : undefined}
            aria-label="系统设置"
            title="系统设置"
          >
            <Icon name="settings" />
          </button>
        </div>
      </header>
      <main className="main-content">{children}</main>
    </div>
  );
}
