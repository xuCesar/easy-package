import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { mockScan } from "../mock-data";
import type { ScanSettings } from "../types";
import { PackagesPage } from "./PackagesPage";

afterEach(cleanup);

const registryScanSettings: ScanSettings = {
  ...mockScan.scanSettings,
  networkPolicy: "registry",
};

function renderPackagesPage({
  scanSettings = registryScanSettings,
  packages = mockScan.packages,
  onNavigate = vi.fn(),
  onStartUpgradePlan = vi.fn(),
}: {
  scanSettings?: ScanSettings;
  packages?: typeof mockScan.packages;
  onNavigate?: ReturnType<typeof vi.fn>;
  onStartUpgradePlan?: ReturnType<typeof vi.fn>;
} = {}) {
  const onUpdateSettings = vi.fn().mockResolvedValue(undefined);
  const onRefresh = vi.fn();

  render(
    <PackagesPage
      packages={packages}
      scanSettings={scanSettings}
      onNavigate={onNavigate}
      onStartUpgradePlan={onStartUpgradePlan}
      onUpdateSettings={onUpdateSettings}
      onRefresh={onRefresh}
    />,
  );

  return { onNavigate, onStartUpgradePlan, onUpdateSettings, onRefresh };
}

describe("PackagesPage", () => {
  it("组合搜索、管理器和更新状态筛选软件包", async () => {
    renderPackagesPage();

    fireEvent.change(screen.getByPlaceholderText("搜索软件包"), { target: { value: "type" } });
    fireEvent.change(screen.getByLabelText("管理器"), { target: { value: "npm" } });
    fireEvent.change(screen.getByLabelText("状态"), { target: { value: "available" } });

    expect(await screen.findByText("typescript")).toBeInTheDocument();
    expect(screen.queryByText("git")).not.toBeInTheDocument();
    expect(screen.getByText("1 个结果")).toBeInTheDocument();
  });

  it("搜索无结果时展示通用空状态", async () => {
    renderPackagesPage();

    fireEvent.change(screen.getByPlaceholderText("搜索软件包"), {
      target: { value: "missing-package" },
    });

    expect(await screen.findByText("没有匹配的软件包")).toBeInTheDocument();
    expect(screen.getByText("调整搜索词或筛选条件后重试。")).toBeInTheDocument();
    expect(screen.queryByText(/当前为离线模式/)).not.toBeInTheDocument();
  });

  it("只读管理器的软件包明确标记为仅查看", () => {
    renderPackagesPage();

    const readonlyRow = screen.getByText("black").closest("tr");
    expect(readonlyRow).not.toBeNull();
    if (!readonlyRow) {
      throw new Error("未找到 pip:black 所在行");
    }
    expect(within(readonlyRow).getByText("仅查看")).toBeInTheDocument();
    expect(within(readonlyRow).queryByRole("button", { name: "升级 black" })).not.toBeInTheDocument();
  });

  it("可写但无需升级的软件包展示可管理状态", () => {
    renderPackagesPage();

    const writableRow = screen.getByText("homebrew:ripgrep").closest("tr");
    expect(writableRow).not.toBeNull();
    if (!writableRow) {
      throw new Error("未找到 homebrew:ripgrep 所在行");
    }
    expect(within(writableRow).getByText("可管理")).toBeInTheDocument();
  });

  it("可写且可更新的软件包可生成单目标升级预填", () => {
    const onStartUpgradePlan = vi.fn();
    renderPackagesPage({ onStartUpgradePlan });

    fireEvent.click(screen.getByRole("button", { name: "升级 git" }));

    expect(onStartUpgradePlan).toHaveBeenCalledWith({
      managerId: "homebrew",
      targets: ["git"],
      truncatedCount: 0,
      otherWritableCount: 0,
      unwritableCount: 0,
    });
  });

  it("批量升级只包含当前筛选中的可写更新项", () => {
    const onStartUpgradePlan = vi.fn();
    renderPackagesPage({ onStartUpgradePlan });

    fireEvent.change(screen.getByLabelText("状态"), { target: { value: "available" } });
    fireEvent.click(screen.getByRole("button", { name: "批量生成升级计划（1）" }));

    expect(onStartUpgradePlan).toHaveBeenCalledWith({
      managerId: "homebrew",
      targets: ["git"],
      truncatedCount: 0,
      otherWritableCount: 1,
      unwritableCount: 1,
    });
  });

  it("当前筛选只有只读更新项时禁用批量升级", () => {
    renderPackagesPage();

    fireEvent.change(screen.getByLabelText("管理器"), { target: { value: "pip" } });

    expect(screen.getByRole("button", { name: "批量生成升级计划" })).toBeDisabled();
  });

  it("可从页头进入管理操作", () => {
    const onNavigate = vi.fn();
    renderPackagesPage({ onNavigate });

    fireEvent.click(screen.getByRole("button", { name: "管理操作" }));

    expect(onNavigate).toHaveBeenCalledWith("actions");
  });

  it("离线时解释可更新空态，并允许用户显式开启检查", async () => {
    const packages = mockScan.packages.map((pkg) => ({ ...pkg, updateStatus: "unknown" as const }));
    const { onUpdateSettings, onRefresh } = renderPackagesPage({
      packages,
      scanSettings: mockScan.scanSettings,
    });

    expect(screen.getByText(/离线模式，不会查询软件包最新版本/)).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("状态"), { target: { value: "available" } });
    expect(screen.getByText(/当前为离线模式，不会查询最新版本/)).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "允许检查更新" }));
    await waitFor(() =>
      expect(onUpdateSettings).toHaveBeenCalledWith({ ...mockScan.scanSettings, networkPolicy: "registry" }),
    );
    expect(onRefresh).not.toHaveBeenCalled();
    expect(await screen.findByText(/需要重新扫描后才会显示可更新状态/)).toBeInTheDocument();
  });
});
