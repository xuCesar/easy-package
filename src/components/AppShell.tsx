import type { ReactNode } from "react";
import { Icon, type IconName } from "./Icon";
import type { PageId } from "../types";

const navItems: Array<{ id: PageId; label: string; icon: IconName }> = [
  { id: "overview", label: "概览", icon: "overview" },
  { id: "packages", label: "软件包", icon: "packages" },
  { id: "projects", label: "项目", icon: "projects" },
  { id: "environment", label: "诊断", icon: "environment" },
];

const diagnosticPages = new Set<PageId>(["environment", "dependencies", "supplyChain", "runtimes", "history", "logs"]);

interface AppShellProps {
  page: PageId;
  onNavigate: (page: PageId) => void;
  children: ReactNode;
}

export function AppShell({ page, onNavigate, children }: AppShellProps) {
  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand"><span className="brand__mark"><Icon name="packages" /></span><span>Easy Package</span></div>
        <nav className="nav" aria-label="主要导航">
          {navItems.map((item) => (
            <button key={item.id} className={`nav__item ${(item.id === "environment" ? diagnosticPages.has(page) : page === item.id) ? "nav__item--active" : ""}`} onClick={() => onNavigate(item.id)} aria-current={(item.id === "environment" ? diagnosticPages.has(page) : page === item.id) ? "page" : undefined}>
              <Icon name={item.icon} />
              <span>{item.label}</span>
            </button>
          ))}
        </nav>
        <div className="sidebar__footer"><button className={page === "settings" ? "sidebar-settings sidebar-settings--active" : "sidebar-settings"} onClick={() => onNavigate("settings")} aria-current={page === "settings" ? "page" : undefined}><Icon name="settings" /><span>系统设置</span></button><div><span>v0.1.0</span><span className="readonly-label">受控模式</span></div></div>
      </aside>
      <main className="main-content">{children}</main>
    </div>
  );
}
