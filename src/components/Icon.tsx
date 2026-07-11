import type { ReactNode, SVGProps } from "react";

export type IconName = "overview" | "packages" | "projects" | "dependencies" | "runtimes" | "history" | "environment" | "refresh" | "search" | "warning" | "check" | "info" | "folder" | "trash" | "copy" | "terminal" | "chevron";

const paths: Record<IconName, ReactNode> = {
  overview: <><path d="M3 10.5 12 3l9 7.5"/><path d="M5 9.5V21h14V9.5M9 21v-7h6v7"/></>,
  packages: <><path d="m12 3 8 4.5v9L12 21l-8-4.5v-9L12 3Z"/><path d="m4.2 7.6 7.8 4.5 7.8-4.5M12 12.1V21"/></>,
  projects: <><path d="M3 6.5h7l2 2h9v10.5H3z"/><path d="M3 6.5V5h7l2 2"/></>,
  dependencies: <><circle cx="6" cy="12" r="2.5"/><circle cx="18" cy="6" r="2.5"/><circle cx="18" cy="18" r="2.5"/><path d="m8.2 10.8 7.6-3.6M8.2 13.2l7.6 3.6"/></>,
  runtimes: <><path d="M7 4h10l3 8-3 8H7l-3-8 3-8Z"/><path d="M9 9h6M9 15h6M12 9v6"/></>,
  history: <><circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/></>,
  environment: <><rect x="3" y="4" width="18" height="16" rx="2"/><path d="m7 9 3 3-3 3M13 15h4"/></>,
  refresh: <><path d="M20 6v5h-5"/><path d="M4 18v-5h5"/><path d="M18.5 9A7 7 0 0 0 6.2 6.2L4 8M5.5 15A7 7 0 0 0 17.8 17.8L20 16"/></>,
  search: <><circle cx="11" cy="11" r="7"/><path d="m20 20-4-4"/></>,
  warning: <><path d="M12 3 2.8 20h18.4L12 3Z"/><path d="M12 9v4M12 17h.01"/></>,
  check: <><circle cx="12" cy="12" r="9"/><path d="m8 12 2.7 2.7L16.5 9"/></>,
  info: <><circle cx="12" cy="12" r="9"/><path d="M12 11v6M12 7h.01"/></>,
  folder: <><path d="M3 6.5h7l2 2h9v10.5H3z"/></>,
  trash: <><path d="M4 7h16M9 7V4h6v3M6 7l1 14h10l1-14M10 11v6M14 11v6"/></>,
  copy: <><rect x="8" y="8" width="12" height="12" rx="2"/><path d="M16 8V4H4v12h4"/></>,
  terminal: <><rect x="3" y="4" width="18" height="16" rx="2"/><path d="m7 9 3 3-3 3M13 15h4"/></>,
  chevron: <path d="m9 18 6-6-6-6"/>,
};

export function Icon({ name, ...props }: { name: IconName } & SVGProps<SVGSVGElement>) {
  return <svg viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" {...props}>{paths[name]}</svg>;
}
