import { Component, type ErrorInfo, type ReactNode } from "react";
import { Button } from "@/components/ui/button";
import { AlertTriangle, RefreshCw } from "lucide-react";
import i18n from "@/i18n";

interface Props {
  children: ReactNode;
}

interface State {
  hasError: boolean;
  error: Error | null;
}

export class ErrorBoundary extends Component<Props, State> {
  constructor(props: Props) {
    super(props);
    this.state = { hasError: false, error: null };
  }

  static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, errorInfo: ErrorInfo) {
    console.error("[ErrorBoundary] Uncaught error:", error, errorInfo);
  }

  handleReload = () => {
    window.location.reload();
  };

  handleReset = () => {
    this.setState({ hasError: false, error: null });
  };

  render() {
    if (this.state.hasError) {
      return (
        <div className="flex flex-col items-center justify-center h-screen p-8 bg-background text-foreground">
          <AlertTriangle className="w-12 h-12 text-destructive mb-4" />
          <h1 className="text-xl font-bold mb-2">{i18n.t("error.title")}</h1>
          <p className="text-sm text-muted-foreground mb-1 max-w-md text-center">
            {i18n.t("error.description")}
          </p>
          {this.state.error && (
            <pre className="text-xs text-destructive bg-destructive/10 rounded p-3 mt-3 max-w-lg overflow-auto max-h-32">
              {this.state.error.message}
            </pre>
          )}
          <div className="flex gap-2 mt-6">
            <Button variant="outline" onClick={this.handleReset}>
              <RefreshCw className="w-4 h-4 mr-2" />
              {i18n.t("error.tryAgain")}
            </Button>
            <Button onClick={this.handleReload}>
              {i18n.t("error.reloadApp")}
            </Button>
          </div>
        </div>
      );
    }

    return this.props.children;
  }
}
