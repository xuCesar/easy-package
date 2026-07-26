import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ProjectsPage } from "./ProjectsPage";
import type { ProjectMetadata } from "../types";

const project: ProjectMetadata = {
  name: "demo-app",
  path: "/tmp/demo-app",
  ecosystems: ["JavaScript"],
  lockFiles: ["pnpm-lock.yaml"],
  runtimeRequirements: [{ runtime: "Node.js", requirement: ">=22" }],
  packageManager: "pnpm@11",
  dependencies: [{ ecosystem: "JavaScript", name: "react", normalizedName: "react", versionRequirement: "^19.0.0", scopes: ["运行"], resolvedVersion: "19.1.1", resolutionSource: "pnpm-lock.yaml", resolutionChecked: true }],
  warnings: [],
};

afterEach(cleanup);

const pageProps = {
  projects: [project],
  workspaces: [],
  scanRoots: [project.path],
  ignoredDirectoryNames: ["node_modules", ".next", ".git", "target", "dist", "build", ".venv", "vendor"],
  onAddRoot: vi.fn().mockResolvedValue(undefined),
  onRemoveRoot: vi.fn().mockResolvedValue(undefined),
  onRefresh: vi.fn(),
};

describe("ProjectsPage", () => {
  it("在浏览器预览中添加和移除扫描目录", async () => {
    const onAddRoot = vi.fn().mockResolvedValue(undefined);
    const onRemoveRoot = vi.fn().mockResolvedValue(undefined);

    render(<ProjectsPage {...pageProps} onAddRoot={onAddRoot} onRemoveRoot={onRemoveRoot} />);

    fireEvent.click(screen.getByRole("button", { name: "添加目录" }));
    await waitFor(() => expect(onAddRoot).toHaveBeenCalledWith("/Users/demo/Code/new-project"));

    fireEvent.click(screen.getByRole("button", { name: `移除扫描目录 ${project.path}` }));
    expect(onRemoveRoot).toHaveBeenCalledWith(project.path);
  });

  it("显示添加扫描目录失败原因", async () => {
    const onAddRoot = vi.fn().mockRejectedValue(new Error("目录不可读取"));

    render(<ProjectsPage {...pageProps} projects={[]} scanRoots={[]} onAddRoot={onAddRoot} />);

    fireEvent.click(screen.getByRole("button", { name: "添加目录" }));
    expect(await screen.findByText("目录不可读取")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "添加目录" })).toBeEnabled();
  });

  it("按项目展示列表并进入详情后返回", () => {
    render(<ProjectsPage {...pageProps} />);

    expect(screen.getByRole("region", { name: "项目列表" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "查看项目 demo-app" }));

    expect(screen.getByRole("heading", { name: "demo-app" })).toBeInTheDocument();
    expect(screen.getByText("react")).toBeInTheDocument();
    expect(screen.getByText("19.1.1", { exact: false })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "返回项目列表" }));
    expect(screen.getByRole("region", { name: "项目列表" })).toBeInTheDocument();
  });

  it("将工作区聚合为一个项目并在详情展示成员", () => {
    const rootProject = { ...project, name: "suite", path: "/tmp/suite" };
    const member = { ...project, name: "@suite/admin", path: "/tmp/suite/apps/admin", workspace: { name: "suite", path: "/tmp/suite", ecosystem: "JavaScript" } };
    render(<ProjectsPage {...pageProps} projects={[rootProject, member]} workspaces={[{ name: "suite", path: "/tmp/suite", ecosystem: "JavaScript", memberPaths: [member.path] }]} scanRoots={[rootProject.path]} />);

    expect(screen.getAllByRole("button", { name: /查看项目/ })).toHaveLength(1);
    fireEvent.click(screen.getByRole("button", { name: "查看项目 suite" }));

    expect(screen.getByRole("heading", { name: "工作区成员" })).toBeInTheDocument();
    expect(screen.getByText("@suite/admin")).toBeInTheDocument();
  });

  it("过滤旧快照中默认忽略目录下的伪项目", () => {
    const rootProject = { ...project, name: "suite", path: "/tmp/suite" };
    const generated = { ...project, name: "types", path: "/tmp/suite/apps/admin/.next/types" };
    render(<ProjectsPage {...pageProps} projects={[rootProject, generated]} scanRoots={[rootProject.path]} ignoredDirectoryNames={["node_modules", ".git", "dist"]} />);

    expect(screen.getByRole("button", { name: "查看项目 suite" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "查看项目 types" })).not.toBeInTheDocument();
  });

  it("为没有项目标记的扫描根目录保留未识别入口", () => {
    render(<ProjectsPage {...pageProps} projects={[]} scanRoots={["/tmp/easy-growth"]} />);

    fireEvent.click(screen.getByRole("button", { name: "查看项目 easy-growth" }));

    expect(screen.getByRole("heading", { name: "easy-growth" })).toBeInTheDocument();
    expect(screen.getByText("未发现受支持的项目标记")).toBeInTheDocument();
  });
});
