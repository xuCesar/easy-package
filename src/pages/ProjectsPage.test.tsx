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
  warnings: [],
};

afterEach(cleanup);

describe("ProjectsPage", () => {
  it("在浏览器预览中添加和移除扫描目录", async () => {
    const onAddRoot = vi.fn().mockResolvedValue(undefined);
    const onRemoveRoot = vi.fn().mockResolvedValue(undefined);

    render(<ProjectsPage projects={[project]} scanRoots={[project.path]} onAddRoot={onAddRoot} onRemoveRoot={onRemoveRoot} onRefresh={vi.fn()} />);

    fireEvent.click(screen.getByRole("button", { name: "添加目录" }));
    await waitFor(() => expect(onAddRoot).toHaveBeenCalledWith("/Users/demo/Code/new-project"));

    fireEvent.click(screen.getByRole("button", { name: `移除扫描目录 ${project.path}` }));
    expect(onRemoveRoot).toHaveBeenCalledWith(project.path);
  });

  it("显示添加扫描目录失败原因", async () => {
    const onAddRoot = vi.fn().mockRejectedValue(new Error("目录不可读取"));

    render(<ProjectsPage projects={[]} scanRoots={[]} onAddRoot={onAddRoot} onRemoveRoot={vi.fn()} onRefresh={vi.fn()} />);

    fireEvent.click(screen.getByRole("button", { name: "添加目录" }));
    expect(await screen.findByText("目录不可读取")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "添加目录" })).toBeEnabled();
  });
});
