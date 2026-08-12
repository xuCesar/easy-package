import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { mockScan } from "../mock-data";
import { PackagesPage } from "./PackagesPage";

afterEach(cleanup);

describe("PackagesPage", () => {
  it("离线时解释可更新空态，并允许用户显式开启检查", async () => {
    const onUpdateSettings = vi.fn().mockResolvedValue(undefined);
    const onRefresh = vi.fn();
    render(
      <PackagesPage
        packages={mockScan.packages.map((pkg) => ({ ...pkg, updateStatus: "unknown" as const }))}
        scanSettings={mockScan.scanSettings}
        onNavigate={vi.fn()}
        onStartUpgradePlan={vi.fn()}
        onUpdateSettings={onUpdateSettings}
        onRefresh={onRefresh}
      />,
    );

    expect(screen.getByText(/离线模式，不会查询软件包最新版本/)).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("状态"), { target: { value: "available" } });
    expect(screen.getByText(/当前为离线模式，不会查询最新版本/)).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "允许检查更新" }));
    await waitFor(() =>
      expect(onUpdateSettings).toHaveBeenCalledWith({ ...mockScan.scanSettings, networkPolicy: "registry" }),
    );
    expect(onRefresh).not.toHaveBeenCalled();
    expect(screen.getByText(/需要重新扫描后才会显示可更新状态/)).toBeInTheDocument();
  });
});
