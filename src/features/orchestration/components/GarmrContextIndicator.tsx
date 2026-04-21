import { useMemo, useState } from "react";
import { Markdown } from "../../messages/components/Markdown";
import type { OrchestrationContextMsg } from "@/services/tauri";
import { estimateContextTokens } from "../utils/buildContext";
import "./GarmrPanel.css";

type GarmrContextIndicatorProps = {
  /** What WILL be forwarded if the user sends now. */
  preview: OrchestrationContextMsg[] | null;
  /** What was forwarded in the most recent send. */
  lastSent?: OrchestrationContextMsg[] | null;
  /** Whether orchestration is currently running. */
  isProcessing?: boolean;
  className?: string;
};

/**
 * Compact pill rendered above the composer showing how much conversation
 * context will be forwarded to the GARMR orchestrator on the next send.
 * Expands on click to reveal the exact transcript.
 *
 * Intentionally minimal: one line collapsed, list when expanded. No extra
 * controls — the source of truth for limit/disable lives in Settings.
 */
export function GarmrContextIndicator({
  preview,
  lastSent,
  isProcessing = false,
  className,
}: GarmrContextIndicatorProps) {
  const [expanded, setExpanded] = useState(false);

  // Prefer showing what's queued for the next send; fall back to last sent
  // so the user can still inspect after a dispatch completes.
  const visible = preview ?? lastSent ?? null;
  const tokens = useMemo(() => estimateContextTokens(visible), [visible]);

  if (!visible || visible.length === 0) {
    return (
      <div
        className={`garmr-context-indicator garmr-context-indicator-empty${className ? ` ${className}` : ""}`}
      >
        <span className="garmr-context-indicator-label">
          GARMR context: <em>new thread — no prior messages</em>
        </span>
      </div>
    );
  }

  const label = isProcessing ? "Sending" : preview ? "Will send" : "Sent";

  return (
    <div className={`garmr-context-indicator${className ? ` ${className}` : ""}`}>
      <button
        type="button"
        className="garmr-context-indicator-button"
        onClick={() => setExpanded((prev) => !prev)}
        aria-expanded={expanded}
        title={
          expanded
            ? "Hide the transcript GARMR sees"
            : "Show the transcript GARMR sees"
        }
      >
        <span className="garmr-context-indicator-chevron" aria-hidden>
          {expanded ? "▾" : "▸"}
        </span>
        <span className="garmr-context-indicator-dot" aria-hidden />
        <span className="garmr-context-indicator-label">
          {label} {visible.length} {visible.length === 1 ? "message" : "messages"}{" "}
          as context
        </span>
        <span className="garmr-context-indicator-meta">
          ~{tokens.toLocaleString()} tokens
        </span>
      </button>
      {expanded && (
        <ol className="garmr-context-indicator-list">
          {visible.map((msg, idx) => (
            <li
              key={`${idx}-${msg.role}`}
              className={`garmr-context-indicator-msg garmr-context-indicator-msg-${msg.role}`}
            >
              <span className="garmr-context-indicator-role">{msg.role}</span>
              <Markdown
                value={msg.text}
                className="garmr-context-indicator-text"
                codeBlockStyle="message"
              />
            </li>
          ))}
        </ol>
      )}
    </div>
  );
}
