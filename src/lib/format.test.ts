import { describe, expect, it } from "vitest";
import { mockScan } from "../mock-data";
import { filterPackages, formatBytes, managerLabel } from "./format";

describe("filterPackages", () => {
  it("按名称、管理器和状态组合过滤", () => {
    expect(
      filterPackages(mockScan.packages, { query: "type", manager: "npm", status: "available" }).map((pkg) => pkg.name),
    ).toEqual(["typescript"]);
  });

  it("空条件返回所有包", () => {
    expect(filterPackages(mockScan.packages, { query: "", manager: "all", status: "all" })).toHaveLength(
      mockScan.packages.length,
    );
  });
});

describe("formatBytes", () => {
  it("格式化存储空间", () => {
    expect(formatBytes(3 * 1024 ** 3)).toBe("3.0 GB");
    expect(formatBytes(undefined)).toBe("—");
  });
});

describe("managerLabel", () => {
  it("提供 RubyGems 与 Composer 的显示名称", () => {
    expect(managerLabel.rubygems).toBe("RubyGems");
    expect(managerLabel.composer).toBe("Composer");
  });
});
