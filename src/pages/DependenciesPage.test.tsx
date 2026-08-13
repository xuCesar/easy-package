import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type {
  DependencyGraphSummary,
  DependencyInsight,
  ProjectAnalysisView,
  ProjectDependencyGraph,
  ProjectMetadata,
  ProjectWorkspace,
} from "../types";
import { ProjectAnalysisPage } from "./ProjectAnalysisPage";

const insights: DependencyInsight[] = [
  {
    ecosystem: "JavaScript",
    name: "react",
    projectCount: 2,
    versionRequirements: ["^18", "^19"],
    resolvedVersions: ["18.3.1", "19.1.1"],
    hasVersionDivergence: true,
    hasResolvedVersionDivergence: true,
    hasResolutionRisk: true,
    hasHealthRisk: true,
    projects: [
      {
        projectName: "web",
        projectPath: "/tmp/web",
        versionRequirement: "^19",
        scopes: ["运行"],
        resolvedVersion: "19.1.1",
        resolutionSource: "package-lock.json",
      },
      {
        projectName: "docs",
        projectPath: "/tmp/docs",
        versionRequirement: "^18",
        scopes: ["开发"],
        resolvedVersion: "18.3.1",
        resolutionSource: "package-lock.json",
      },
    ],
  },
  {
    ecosystem: "Python",
    name: "httpx",
    projectCount: 1,
    versionRequirements: [">=0.28"],
    resolvedVersions: ["0.28.1"],
    hasVersionDivergence: false,
    hasResolvedVersionDivergence: false,
    hasResolutionRisk: false,
    hasHealthRisk: false,
    projects: [
      {
        projectName: "api",
        projectPath: "/tmp/api",
        versionRequirement: ">=0.28",
        scopes: ["运行"],
        resolvedVersion: "0.28.1",
        resolutionSource: "uv.lock",
      },
    ],
  },
];

const graphSummary: DependencyGraphSummary = {
  nodeCount: 4,
  edgeCount: 3,
  directCount: 2,
  transitiveCount: 1,
  duplicateVersionCount: 1,
  unreachableCount: 0,
  cycleCount: 0,
  completeness: "complete",
  sources: ["package-lock.json"],
  sourceDigest: "abc123",
};

const projects: ProjectMetadata[] = [
  {
    name: "web",
    path: "/tmp/web",
    ecosystems: ["JavaScript"],
    lockFiles: ["package-lock.json"],
    runtimeRequirements: [],
    packageManager: "npm",
    dependencies: [],
    dependencyGraphSummary: graphSummary,
    warnings: [],
  },
];

const graph: ProjectDependencyGraph = {
  projectName: "web",
  projectPath: "/tmp/web",
  completeness: "complete",
  sources: ["package-lock.json"],
  sourceDigest: "abc123",
  nodes: [
    { id: "root", ecosystem: "Project", name: "web", version: "", kind: "project", direct: false, scopes: [] },
    {
      id: "react-19",
      ecosystem: "JavaScript",
      name: "react",
      version: "19.1.1",
      kind: "package",
      direct: true,
      scopes: ["运行"],
    },
    {
      id: "scheduler",
      ecosystem: "JavaScript",
      name: "scheduler",
      version: "0.26.0",
      kind: "package",
      direct: false,
      scopes: ["运行"],
    },
    {
      id: "react-18",
      ecosystem: "JavaScript",
      name: "react",
      version: "18.3.1",
      kind: "package",
      direct: true,
      scopes: ["开发"],
    },
  ],
  edges: [
    { from: "root", to: "react-19", dependencyType: "运行" },
    { from: "react-19", to: "scheduler", dependencyType: "运行" },
    { from: "root", to: "react-18", dependencyType: "开发" },
  ],
  warnings: [],
  summary: graphSummary,
};

interface HarnessProps {
  insights: DependencyInsight[];
  projects: ProjectMetadata[];
  workspaces?: ProjectWorkspace[];
  scanRoots?: string[];
  ignoredDirectoryNames?: string[];
  onLoadGraph: (projectPath: string) => Promise<ProjectDependencyGraph>;
  onExportSbom: ReturnType<typeof vi.fn>;
}

function Harness({ workspaces = [], scanRoots = [], ignoredDirectoryNames = [], ...props }: HarnessProps) {
  const [view, setView] = useState<ProjectAnalysisView>("index");
  return (
    <ProjectAnalysisPage
      view={view}
      onChangeView={setView}
      workspaces={workspaces}
      scanRoots={scanRoots}
      ignoredDirectoryNames={ignoredDirectoryNames}
      onLoadReport={vi.fn()}
      {...props}
    />
  );
}

function renderPage(overrides: Partial<HarnessProps> = {}) {
  const props: HarnessProps = {
    insights,
    projects,
    onLoadGraph: vi.fn().mockResolvedValue(graph),
    onExportSbom: vi.fn().mockResolvedValue({ saved: true }),
    ...overrides,
  };
  return { ...render(<Harness {...props} />), props };
}

afterEach(cleanup);

describe("DependenciesPage", () => {
  it("按名称、生态和版本分歧筛选依赖，并展示项目明细", () => {
    renderPage();
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

  it("在全局依赖视图和依赖图中按工作区聚合成员，并默认隐藏生成目录", () => {
    const workspaceRoot = { ...projects[0], name: "easy-mes", path: "/tmp/easy-mes" };
    const workspaceMember = {
      ...projects[0],
      name: "@easy-mes/admin",
      path: "/tmp/easy-mes/apps/admin",
      workspace: { name: "easy-mes", path: "/tmp/easy-mes", ecosystem: "JavaScript" },
    };
    const generated = { ...projects[0], name: ".next", path: "/tmp/easy-mes/apps/admin/.next" };
    const workspaceInsights: DependencyInsight[] = [
      {
        ...insights[0],
        projectCount: 3,
        projects: [
          {
            projectName: "easy-mes",
            projectPath: workspaceRoot.path,
            versionRequirement: "^19",
            scopes: ["运行"],
            resolvedVersion: "19.1.1",
            resolutionSource: "pnpm-lock.yaml",
          },
          {
            projectName: "@easy-mes/admin",
            projectPath: workspaceMember.path,
            versionRequirement: "^19",
            scopes: ["开发"],
            resolvedVersion: "19.1.1",
            resolutionSource: "pnpm-lock.yaml",
          },
          {
            projectName: ".next",
            projectPath: generated.path,
            versionRequirement: "^19",
            scopes: ["运行"],
            resolvedVersion: "19.1.1",
            resolutionSource: "pnpm-lock.yaml",
          },
        ],
      },
    ];
    renderPage({
      insights: workspaceInsights,
      projects: [workspaceRoot, workspaceMember, generated],
      workspaces: [
        {
          name: "easy-mes",
          path: workspaceRoot.path,
          ecosystem: "JavaScript",
          memberPaths: [workspaceRoot.path, workspaceMember.path],
        },
      ],
      scanRoots: ["/tmp"],
      ignoredDirectoryNames: [".next"],
    });

    expect(screen.getByText("easy-mes · 2 个引用项目")).toBeInTheDocument();
    expect(screen.queryByText(".next · 已忽略")).not.toBeInTheDocument();
    expect(screen.getByText("已索引项目").parentElement?.querySelector("strong")).toHaveTextContent("1");

    fireEvent.click(screen.getByRole("tab", { name: "完整依赖图" }));
    expect(screen.getByRole("option", { name: "easy-mes · 工作区 · 完整" })).toBeInTheDocument();
    expect(screen.queryByRole("option", { name: /@easy-mes\/admin/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("option", { name: /\.next/ })).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("checkbox", { name: "显示已忽略的生成目录" }));
    expect(screen.getByRole("option", { name: ".next · 已忽略 · 完整" })).toBeInTheDocument();
  });

  it("展示 JavaScript、Go、Ruby、PHP 与 Poetry 锁文件解析来源", () => {
    const ecosystemInsights: DependencyInsight[] = [
      {
        ecosystem: "JavaScript",
        name: "hono",
        projectCount: 1,
        versionRequirements: ["^4.6"],
        resolvedVersions: ["4.6.14"],
        hasVersionDivergence: false,
        hasResolvedVersionDivergence: false,
        hasResolutionRisk: false,
        hasHealthRisk: false,
        projects: [
          {
            projectName: "bun",
            projectPath: "/tmp/bun",
            versionRequirement: "^4.6",
            scopes: ["运行"],
            resolvedVersion: "4.6.14",
            resolutionSource: "bun.lock",
          },
        ],
      },
      {
        ecosystem: "JavaScript",
        name: "zod",
        projectCount: 1,
        versionRequirements: ["^3.24"],
        resolvedVersions: ["3.24.1"],
        hasVersionDivergence: false,
        hasResolvedVersionDivergence: false,
        hasResolutionRisk: false,
        hasHealthRisk: false,
        projects: [
          {
            projectName: "yarn",
            projectPath: "/tmp/yarn",
            versionRequirement: "^3.24",
            scopes: ["运行"],
            resolvedVersion: "3.24.1",
            resolutionSource: "yarn.lock",
          },
        ],
      },
      {
        ecosystem: "Go",
        name: "github.com/spf13/cobra",
        projectCount: 1,
        versionRequirements: ["v1.8.1"],
        resolvedVersions: ["v1.8.1"],
        hasVersionDivergence: false,
        hasResolvedVersionDivergence: false,
        hasResolutionRisk: false,
        hasHealthRisk: false,
        projects: [
          {
            projectName: "tool",
            projectPath: "/tmp/tool",
            versionRequirement: "v1.8.1",
            scopes: ["运行"],
            resolvedVersion: "v1.8.1",
            resolutionSource: "go.mod",
          },
        ],
      },
      {
        ecosystem: "Python",
        name: "pytest",
        projectCount: 1,
        versionRequirements: ["^8"],
        resolvedVersions: ["8.3.4"],
        hasVersionDivergence: false,
        hasResolvedVersionDivergence: false,
        hasResolutionRisk: false,
        hasHealthRisk: false,
        projects: [
          {
            projectName: "poetry",
            projectPath: "/tmp/poetry",
            versionRequirement: "^8",
            scopes: ["开发:dev"],
            resolvedVersion: "8.3.4",
            resolutionSource: "poetry.lock",
          },
        ],
      },
      {
        ecosystem: "Ruby",
        name: "rails",
        projectCount: 1,
        versionRequirements: ["~> 8.0"],
        resolvedVersions: ["8.0.1"],
        hasVersionDivergence: false,
        hasResolvedVersionDivergence: false,
        hasResolutionRisk: false,
        hasHealthRisk: false,
        projects: [
          {
            projectName: "rails",
            projectPath: "/tmp/rails",
            versionRequirement: "~> 8.0",
            scopes: ["运行"],
            resolvedVersion: "8.0.1",
            resolutionSource: "Gemfile.lock",
          },
        ],
      },
      {
        ecosystem: "PHP",
        name: "symfony/http-foundation",
        projectCount: 1,
        versionRequirements: ["^7.2"],
        resolvedVersions: ["v7.2.1"],
        hasVersionDivergence: false,
        hasResolvedVersionDivergence: false,
        hasResolutionRisk: false,
        hasHealthRisk: false,
        projects: [
          {
            projectName: "api",
            projectPath: "/tmp/api",
            versionRequirement: "^7.2",
            scopes: ["运行"],
            resolvedVersion: "v7.2.1",
            resolutionSource: "composer.lock",
          },
        ],
      },
    ];
    renderPage({ insights: ecosystemInsights });
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

  it("按需加载项目依赖图并筛选节点、展示最短路径", async () => {
    const { props } = renderPage();
    fireEvent.click(screen.getByRole("tab", { name: "完整依赖图" }));
    expect(screen.getByText("尚未解析项目依赖图")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "解析依赖图" }));
    await waitFor(() => expect(props.onLoadGraph).toHaveBeenCalledWith("/tmp/web"));
    expect(screen.getByText("scheduler")).toBeInTheDocument();

    fireEvent.change(screen.getByLabelText("依赖图范围"), { target: { value: "transitive" } });
    expect(screen.getByText("scheduler")).toBeInTheDocument();
    expect(screen.queryByText("18.3.1")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /scheduler/ }));
    expect(screen.getByText("web → react → scheduler")).toBeInTheDocument();

    fireEvent.change(screen.getByLabelText("依赖图范围"), { target: { value: "duplicates" } });
    expect(screen.getByText("18.3.1")).toBeInTheDocument();
    expect(screen.queryByText("scheduler")).not.toBeInTheDocument();
  });

  it("依赖图与锁文件问题共享同一项目选择", () => {
    const docs: ProjectMetadata = {
      ...projects[0],
      name: "docs",
      path: "/tmp/docs",
      dependencyGraphSummary: undefined,
    };
    renderPage({ projects: [...projects, docs] });

    fireEvent.click(screen.getByRole("tab", { name: "完整依赖图" }));
    fireEvent.change(screen.getByLabelText("依赖图项目"), { target: { value: "/tmp/docs" } });

    fireEvent.click(screen.getByRole("tab", { name: "锁文件问题" }));
    expect(screen.getByLabelText("锁文件问题项目")).toHaveValue("/tmp/docs");

    fireEvent.click(screen.getByRole("tab", { name: "完整依赖图" }));
    expect(screen.getByLabelText("依赖图项目")).toHaveValue("/tmp/docs");
  });

  it("展示 SBOM 导出的成功、取消与失败反馈", async () => {
    const onExportSbom = vi
      .fn()
      .mockResolvedValueOnce({ saved: true })
      .mockResolvedValueOnce({ saved: false })
      .mockRejectedValueOnce(new Error("保存失败"));
    renderPage({ onExportSbom });
    fireEvent.click(screen.getByRole("tab", { name: "完整依赖图" }));
    fireEvent.click(screen.getByRole("button", { name: "解析依赖图" }));
    const exportButton = await screen.findByRole("button", { name: "导出 CycloneDX SBOM" });

    fireEvent.click(exportButton);
    expect(await screen.findByText("CycloneDX SBOM 已导出。")).toBeInTheDocument();
    fireEvent.click(exportButton);
    expect(await screen.findByText("已取消导出。")).toBeInTheDocument();
    fireEvent.click(exportButton);
    expect(await screen.findByText("保存失败")).toBeInTheDocument();
  });

  it("对不支持和无效依赖图禁用 SBOM 导出", async () => {
    const unsupported = {
      ...graph,
      completeness: "unsupported" as const,
      nodes: [graph.nodes[0]],
      edges: [],
      sources: [],
      summary: { ...graph.summary, completeness: "unsupported" as const },
    };
    renderPage({ onLoadGraph: vi.fn().mockResolvedValue(unsupported) });
    fireEvent.click(screen.getByRole("tab", { name: "完整依赖图" }));
    fireEvent.click(screen.getByRole("button", { name: "解析依赖图" }));
    expect(await screen.findByText("不支持")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "导出 CycloneDX SBOM" })).toBeDisabled();
  });
});
