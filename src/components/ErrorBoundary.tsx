import { Component, type ErrorInfo, Fragment, type ReactNode } from "react";
import { Icon } from "./Icon";

interface ErrorBoundaryProps {
  children: ReactNode;
}

interface ErrorBoundaryState {
  error?: Error;
  resetKey: number;
}

// React 错误边界仅支持类组件（getDerivedStateFromError / componentDidCatch）。
export class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  state: ErrorBoundaryState = { resetKey: 0 };

  static getDerivedStateFromError(error: Error): Partial<ErrorBoundaryState> {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo): void {
    console.error("页面渲染异常", error, info.componentStack);
  }

  reset = (): void => {
    this.setState((state) => ({ error: undefined, resetKey: state.resetKey + 1 }));
  };

  reload = (): void => {
    window.location.reload();
  };

  render(): ReactNode {
    if (this.state.error) {
      return (
        <div className="app-shell">
          <main className="main-content">
            <div className="app-state app-state--error" role="alert">
              <Icon name="warning" />
              <h1>界面渲染出现异常</h1>
              <p>{this.state.error.message || "发生未知渲染错误，应用界面已停止更新。"}</p>
              <div className="app-state__actions">
                <button className="button button--primary" onClick={this.reset}>
                  返回概览
                </button>
                <button className="button button--secondary" onClick={this.reload}>
                  重新加载
                </button>
              </div>
            </div>
          </main>
        </div>
      );
    }
    return <Fragment key={this.state.resetKey}>{this.props.children}</Fragment>;
  }
}
