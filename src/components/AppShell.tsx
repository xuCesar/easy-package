import type { ReactNode } from "react";
import type { PageId } from "../types";
import { Icon, type IconName } from "./Icon";

const localNavItems: Array<{ id: PageId; label: string; icon: IconName }> = [
  { id: "overview", label: "概览", icon: "overview" },
  { id: "packages", label: "软件包", icon: "packages" },
  { id: "actions", label: "操作", icon: "terminal" },
  { id: "environment", label: "环境", icon: "environment" },
];

const projectNavItems: Array<{ id: PageId; label: string; icon: IconName }> = [
  { id: "projects", label: "项目", icon: "projects" },
];

const environmentPages = new Set<PageId>(["environment", "runtimes", "history", "logs"]);
const projectPages = new Set<PageId>(["projects"]);

function isActivePage(item: PageId, page: PageId): boolean {
  if (item === "environment") return environmentPages.has(page);
  if (item === "projects") return projectPages.has(page);
  return item === page;
}

interface AppShellProps {
  page: PageId;
  isNavigationReady: boolean;
  onNavigate: (page: PageId) => void;
  children: ReactNode;
}

export function AppShell({ page, isNavigationReady, onNavigate, children }: AppShellProps) {
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
            <div className="nav__group" role="group" aria-label="本机工作区">
              {localNavItems.map((item) => (
                <button
                  key={item.id}
                  className={`nav__item ${isActivePage(item.id, page) ? "nav__item--active" : ""}`}
                  onClick={() => onNavigate(item.id)}
                  disabled={!isNavigationReady && item.id !== "overview"}
                  aria-current={isActivePage(item.id, page) ? "page" : undefined}
                  title={!isNavigationReady && item.id !== "overview" ? "完成首次扫描后可用" : undefined}
                >
                  <Icon name={item.icon} className="nav__icon" />
                  <span>{item.label}</span>
                </button>
              ))}
            </div>
            <span className="nav__divider" aria-hidden="true" />
            <div className="nav__group" role="group" aria-label="项目工作区">
              {projectNavItems.map((item) => (
                <button
                  key={item.id}
                  className={`nav__item ${isActivePage(item.id, page) ? "nav__item--active" : ""}`}
                  onClick={() => onNavigate(item.id)}
                  disabled={!isNavigationReady}
                  aria-current={isActivePage(item.id, page) ? "page" : undefined}
                  title={!isNavigationReady ? "完成首次扫描后可用" : undefined}
                >
                  <Icon name={item.icon} className="nav__icon" />
                  <span>{item.label}</span>
                </button>
              ))}
            </div>
          </nav>
        </div>
        <div className="app-topbar__actions">
          <button
            className="safe-mode-button"
            onClick={() => onNavigate("actions")}
            disabled={!isNavigationReady}
            aria-label="安全操作模式"
            title={isNavigationReady ? "打开操作中心" : "完成首次扫描后可用"}
          >
            <span className="safe-mode-button__dot" />
            <Icon name="terminal" className="safe-mode-button__icon" />
            <span>安全操作模式</span>
          </button>
          <button
            className={page === "settings" ? "topbar-settings topbar-settings--active" : "topbar-settings"}
            onClick={() => onNavigate("settings")}
            disabled={!isNavigationReady}
            aria-current={page === "settings" ? "page" : undefined}
            aria-label="系统设置"
            title={isNavigationReady ? "系统设置" : "完成首次扫描后可用"}
          >
            <Icon name="settings" />
          </button>
        </div>
      </header>
      <main className="main-content">{children}</main>
    </div>
  );
}
