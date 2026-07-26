import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ErrorBoundary } from "./ErrorBoundary";

afterEach(cleanup);

function Bomb({ label }: { label: string }) {
  const [exploded, setExploded] = useState(true);
  if (exploded) {
    throw new Error("渲染爆炸");
  }
  return <button onClick={() => setExploded(true)}>{label}</button>;
}

function Safe() {
  return <h1>安全内容</h1>;
}

describe("ErrorBoundary", () => {
  it("子组件抛出异常时展示 fallback 而非整体崩溃", () => {
    const consoleError = vi.spyOn(console, "error").mockImplementation(() => {});
    render(
      <ErrorBoundary>
        <Bomb label="bomb" />
      </ErrorBoundary>,
    );
    expect(screen.getByRole("alert")).toBeInTheDocument();
    expect(screen.getByText("界面渲染出现异常")).toBeInTheDocument();
    expect(screen.getByText("渲染爆炸")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "返回概览" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "重新加载" })).toBeInTheDocument();
    consoleError.mockRestore();
  });

  it("「返回概览」重置边界并重挂载子树", () => {
    const consoleError = vi.spyOn(console, "error").mockImplementation(() => {});
    let shouldThrow = true;
    function Recoverable() {
      if (shouldThrow) {
        throw new Error("一次性异常");
      }
      return <Safe />;
    }
    render(
      <ErrorBoundary>
        <Recoverable />
      </ErrorBoundary>,
    );
    expect(screen.getByRole("alert")).toBeInTheDocument();
    shouldThrow = false;
    fireEvent.click(screen.getByRole("button", { name: "返回概览" }));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(screen.getByText("安全内容")).toBeInTheDocument();
    consoleError.mockRestore();
  });

  it("「重新加载」触发 window.location.reload", () => {
    const consoleError = vi.spyOn(console, "error").mockImplementation(() => {});
    const reload = vi.fn();
    const original = window.location;
    Object.defineProperty(window, "location", { value: { ...original, reload }, writable: true });
    render(
      <ErrorBoundary>
        <Bomb label="bomb" />
      </ErrorBoundary>,
    );
    fireEvent.click(screen.getByRole("button", { name: "重新加载" }));
    expect(reload).toHaveBeenCalledTimes(1);
    Object.defineProperty(window, "location", { value: original, writable: true });
    consoleError.mockRestore();
  });
});
