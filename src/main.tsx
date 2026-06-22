import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "./index.css";
import App from "./App";
import { ErrorBoundary } from "./ErrorBoundary";
import { ToastProvider } from "./contexts/ToastContext";
import { error as logError, warn as logWarn } from "@tauri-apps/plugin-log";

const serializeConsoleArguments = (values: unknown[]) => values.map(value => {
  if (value instanceof Error) return `${value.name}: ${value.message}\n${value.stack || ''}`;
  if (typeof value === 'string') return value;
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}).join(' ');

const redactConsoleMessage = (message: string) => message
  .replace(/(authorization|cookie|password|token|secret)\s*[:=]\s*([^\s,;]+)/gi, '$1=[redacted]')
  .replace(/(https?:\/\/[^\s?]+)\?[^\s]+/g, '$1?[redacted]');

const originalConsoleError = console.error.bind(console);
const originalConsoleWarn = console.warn.bind(console);
console.error = (...values: unknown[]) => {
  originalConsoleError(...values);
  void logError(redactConsoleMessage(serializeConsoleArguments(values))).catch(() => undefined);
};
console.warn = (...values: unknown[]) => {
  originalConsoleWarn(...values);
  void logWarn(redactConsoleMessage(serializeConsoleArguments(values))).catch(() => undefined);
};

const rootElement = document.getElementById("root");
if (rootElement) {
  createRoot(rootElement).render(
    <StrictMode>
      <ErrorBoundary>
        <ToastProvider>
          <App />
        </ToastProvider>
      </ErrorBoundary>
    </StrictMode>,
  );
}
