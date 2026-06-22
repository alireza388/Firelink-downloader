import { Component, ErrorInfo, ReactNode } from "react";

interface Props {
  children?: ReactNode;
}

interface State {
  hasError: boolean;
  error: Error | null;
  errorInfo: ErrorInfo | null;
}

export class ErrorBoundary extends Component<Props, State> {
  public state: State = {
    hasError: false,
    error: null,
    errorInfo: null
  };

  public static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error, errorInfo: null };
  }

  public componentDidCatch(error: Error, errorInfo: ErrorInfo) {
    console.error("Uncaught error:", error, errorInfo);
    this.setState({ errorInfo });
  }

  public render() {
    if (this.state.hasError) {
      return (
        <div className="flex h-screen w-screen items-center justify-center bg-main-bg p-8 text-text-primary">
          <div className="app-card max-w-lg space-y-4 p-6 text-center">
            <h1 className="text-xl font-semibold">Firelink could not display this window.</h1>
            <p className="text-sm text-text-secondary">
              The error was written to Logs. Reload the interface to reconnect to the running download service.
            </p>
            <button
              type="button"
              className="app-button app-button-primary px-4"
              onClick={() => window.location.reload()}
            >
              Reload Firelink
            </button>
          </div>
        </div>
      );
    }

    return this.props.children;
  }
}
