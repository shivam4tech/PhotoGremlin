import { Component, type ErrorInfo, type ReactNode } from "react";

type Props = {
  children: ReactNode;
  onError: (error: Error, info: ErrorInfo) => void;
};

type State = { failed: boolean };

/** Keeps an exception in one view from blanking the entire application shell. */
export class ErrorBoundary extends Component<Props, State> {
  state: State = { failed: false };

  static getDerivedStateFromError(): State {
    return { failed: true };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    this.props.onError(error, info);
  }

  render() {
    if (this.state.failed) {
      return (
        <section className="empty-state" role="alert">
          <h2>This view could not be displayed.</h2>
          <p>The technical details were recorded in the local log.</p>
          <button className="btn" onClick={() => this.setState({ failed: false })}>
            Try again
          </button>
        </section>
      );
    }
    return this.props.children;
  }
}
