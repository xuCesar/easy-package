import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { mockScan } from "../mock-data";
import { OverviewPage } from "./OverviewPage";

describe("OverviewPage", () => {
  it("没有扫描项目时展示添加入口", () => {
    const onNavigate = vi.fn();
    render(<OverviewPage data={{ ...mockScan, projects: [] }} isLoading={false} onRefresh={vi.fn()} onCancel={vi.fn()} onNavigate={onNavigate} />);

    fireEvent.click(screen.getByRole("button", { name: "添加扫描目录" }));

    expect(onNavigate).toHaveBeenCalledWith("projects");
  });
});
