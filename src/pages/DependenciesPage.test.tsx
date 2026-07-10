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

  it("展示 JavaScript、Go、Ruby、PHP 与 Poetry 锁文件解析来源", () => {
    const ecosystemInsights: DependencyInsight[] = [
      { ecosystem: "JavaScript", name: "hono", projectCount: 1, versionRequirements: ["^4.6"], resolvedVersions: ["4.6.14"], hasVersionDivergence: false, hasResolvedVersionDivergence: false, hasResolutionRisk: false, hasHealthRisk: false, projects: [{ projectName: "bun", projectPath: "/tmp/bun", versionRequirement: "^4.6", scopes: ["运行"], resolvedVersion: "4.6.14", resolutionSource: "bun.lock" }] },
      { ecosystem: "JavaScript", name: "zod", projectCount: 1, versionRequirements: ["^3.24"], resolvedVersions: ["3.24.1"], hasVersionDivergence: false, hasResolvedVersionDivergence: false, hasResolutionRisk: false, hasHealthRisk: false, projects: [{ projectName: "yarn", projectPath: "/tmp/yarn", versionRequirement: "^3.24", scopes: ["运行"], resolvedVersion: "3.24.1", resolutionSource: "yarn.lock" }] },
      { ecosystem: "Go", name: "github.com/spf13/cobra", projectCount: 1, versionRequirements: ["v1.8.1"], resolvedVersions: ["v1.8.1"], hasVersionDivergence: false, hasResolvedVersionDivergence: false, hasResolutionRisk: false, hasHealthRisk: false, projects: [{ projectName: "tool", projectPath: "/tmp/tool", versionRequirement: "v1.8.1", scopes: ["运行"], resolvedVersion: "v1.8.1", resolutionSource: "go.mod" }] },
      { ecosystem: "Python", name: "pytest", projectCount: 1, versionRequirements: ["^8"], resolvedVersions: ["8.3.4"], hasVersionDivergence: false, hasResolvedVersionDivergence: false, hasResolutionRisk: false, hasHealthRisk: false, projects: [{ projectName: "poetry", projectPath: "/tmp/poetry", versionRequirement: "^8", scopes: ["开发:dev"], resolvedVersion: "8.3.4", resolutionSource: "poetry.lock" }] },
      { ecosystem: "Ruby", name: "rails", projectCount: 1, versionRequirements: ["~> 8.0"], resolvedVersions: ["8.0.1"], hasVersionDivergence: false, hasResolvedVersionDivergence: false, hasResolutionRisk: false, hasHealthRisk: false, projects: [{ projectName: "rails", projectPath: "/tmp/rails", versionRequirement: "~> 8.0", scopes: ["运行"], resolvedVersion: "8.0.1", resolutionSource: "Gemfile.lock" }] },
      { ecosystem: "PHP", name: "symfony/http-foundation", projectCount: 1, versionRequirements: ["^7.2"], resolvedVersions: ["v7.2.1"], hasVersionDivergence: false, hasResolvedVersionDivergence: false, hasResolutionRisk: false, hasHealthRisk: false, projects: [{ projectName: "api", projectPath: "/tmp/api", versionRequirement: "^7.2", scopes: ["运行"], resolvedVersion: "v7.2.1", resolutionSource: "composer.lock" }] },
    ];
    render(<DependenciesPage insights={ecosystemInsights} />);
    fireEvent.change(screen.getByLabelText("依赖生态"), { target: { value: "JavaScript" } });
    expect(screen.getByText("bun.lock")).toBeInTheDocument();
    expect(screen.getByText("yarn.lock")).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("依赖生态"), { target: { value: "Go" } });
    expect(screen.getByText("github.com/spf13/cobra")).toBeInTheDocument();
    expect(screen.getByText("go.mod")).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("依赖生态"), { target: { value: "Python" } });
    expect(screen.getByText("poetry.lock")).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("依赖生态"), { target: { value: "Ruby" } });
    expect(screen.getByText("Gemfile.lock")).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("依赖生态"), { target: { value: "PHP" } });
    expect(screen.getByText("composer.lock")).toBeInTheDocument();
  });
});
