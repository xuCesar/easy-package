import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { DependenciesPage } from "./DependenciesPage";
import type { DependencyInsight } from "../types";

const insights: DependencyInsight[] = [
  { ecosystem: "JavaScript", name: "react", projectCount: 2, versionRequirements: ["^18", "^19"], resolvedVersions: ["18.3.1", "19.1.1"], hasVersionDivergence: true, hasResolvedVersionDivergence: true, hasResolutionRisk: true, hasHealthRisk: true, projects: [{ projectName: "web", projectPath: "/tmp/web", versionRequirement: "^19", scopes: ["运行"], resolvedVersion: "19.1.1", resolutionSource: "package-lock.json" }, { projectName: "docs", projectPath: "/tmp/docs", versionRequirement: "^18", scopes: ["开发"], resolvedVersion: "18.3.1", resolutionSource: "package-lock.json" }] },
  { ecosystem: "Python", name: "httpx", projectCount: 1, versionRequirements: [">=0.28"], resolvedVersions: ["0.28.1"], hasVersionDivergence: false, hasResolvedVersionDivergence: false, hasResolutionRisk: false, hasHealthRisk: false, projects: [{ projectName: "api", projectPath: "/tmp/api", versionRequirement: ">=0.28", scopes: ["运行"], resolvedVersion: "0.28.1", resolutionSource: "uv.lock" }] },
];

afterEach(cleanup);

describe("DependenciesPage", () => {
  it("按名称、生态和版本分歧筛选依赖，并展示项目明细", () => {
    render(<DependenciesPage insights={insights} />);
    expect(screen.getByText("docs")).toBeInTheDocument();
    fireEvent.change(screen.getByPlaceholderText("搜索依赖名称"), { target: { value: "http" } });
    fireEvent.change(screen.getByLabelText("依赖生态"), { target: { value: "Python" } });
    expect(screen.getByText("httpx")).toBeInTheDocument();
    expect(screen.queryByText("react")).not.toBeInTheDocument();

    fireEvent.change(screen.getByPlaceholderText("搜索依赖名称"), { target: { value: "" } });
    fireEvent.change(screen.getByLabelText("依赖生态"), { target: { value: "all" } });
    fireEvent.click(screen.getByLabelText("仅看版本分歧"));
    expect(screen.getByText("react")).toBeInTheDocument();
    expect(screen.queryByText("httpx")).not.toBeInTheDocument();
    fireEvent.click(screen.getByLabelText("仅看健康风险"));
    expect(screen.getByText("react")).toBeInTheDocument();
    fireEvent.click(screen.getByLabelText("仅看解析异常"));
    expect(screen.getByText("^18 → 18.3.1")).toBeInTheDocument();
  });
});
