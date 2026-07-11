import type { ReactNode } from "react";
import { Icon, type IconName } from "./Icon";
import type { PageId } from "../types";

const navItems: Array<{ id: PageId; label: string; icon: IconName }> = [
  { id: "overview", label: "概览", icon: "overview" },
  { id: "packages", label: "软件包", icon: "packages" },
  { id: "projects", label: "项目", icon: "projects" },
  { id: "dependencies", label: "依赖", icon: "dependencies" },
  { id: "runtimes", label: "运行时", icon: "runtimes" },
  { id: "history", label: "历史", icon: "history" },
  { id: "environment", label: "环境", icon: "environment" },
];

interface AppShellProps {
  page: PageId;
  onNavigate: (page: PageId) => void;
  children: ReactNode;
}

export function AppShell({ page, onNavigate, children }: AppShellProps) {
  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="window-controls" aria-hidden="true"><i /><i /><i /></div>
        <div className="brand"><span className="brand__mark"><Icon name="terminal" /></span><span>Easy Package</span></div>
        <nav className="nav" aria-label="主要导航">
          {navItems.map((item) => (
            <button key={item.id} className={`nav__item ${page === item.id ? "nav__item--active" : ""}`} onClick={() => onNavigate(item.id)} aria-current={page === item.id ? "page" : undefined}>
              <Icon name={item.icon} />
              <span>{item.label}</span>
            </button>
          ))}
        </nav>
        <div className="sidebar__footer"><span>v0.1.0</span><span className="readonly-label">只读模式</span></div>
      </aside>
      <main className="main-content">{children}</main>
    </div>
  );
}
