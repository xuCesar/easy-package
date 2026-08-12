import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { ScanSettings } from "../types";
import { RegistryPolicyNotice } from "./RegistryPolicyNotice";

const scanSettings: ScanSettings = {
  ignoredPaths: [],
  maxDepth: 6,
  defaultIgnoredDirectoryNames: [],
  networkPolicy: "offline",
};

afterEach(cleanup);

describe("RegistryPolicyNotice", () => {
  it("只切换联网策略，并将重新扫描作为独立确认动作", async () => {
    const onUpdateSettings = vi.fn().mockResolvedValue(undefined);
    const onRefresh = vi.fn();
    render(
      <RegistryPolicyNotice scanSettings={scanSettings} onUpdateSettings={onUpdateSettings} onRefresh={onRefresh} />,
    );

    fireEvent.click(screen.getByRole("button", { name: "允许检查更新" }));

    await waitFor(() => expect(onUpdateSettings).toHaveBeenCalledWith({ ...scanSettings, networkPolicy: "registry" }));
    expect(onRefresh).not.toHaveBeenCalled();
    expect(screen.getByRole("status")).toHaveTextContent("需要重新扫描后才会显示可更新状态");

    fireEvent.click(screen.getByRole("button", { name: "重新扫描" }));
    expect(onRefresh).toHaveBeenCalledOnce();
  });
});
