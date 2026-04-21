import { useEffect, useMemo, useRef, useState } from "react";
import { Markdown } from "../../messages/components/Markdown";
import type { OrchestrationContextMsg } from "@/services/tauri";
import { estimateContextTokens } from "../utils/buildContext";
import "./GarmrPanel.css";

type GarmrMessage = {
  id: string;
  role: "user" | "assistant";
  text: string;
  timestamp: number;
};

type GarmrPanelProps = {
  messages: GarmrMessage[];
  isProcessing: boolean;
  onClose: () => void;
  /** Transcript that was last forwarded to the orchestrator (read-only). */
  lastContextSent?: OrchestrationContextMsg[] | null;
  /** Preview of what WILL be sent if the user hits send now. */
  previewContext?: OrchestrationContextMsg[] | null;
};

export function GarmrPanel({
  messages,
  isProcessing,
  onClose,
  lastContextSent,
  previewContext,
}: GarmrPanelProps) {
  const bottomRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages]);

  const hasAnything =
    messages.length > 0 ||
    isProcessing ||
    (lastContextSent && lastContextSent.length > 0) ||
    (previewContext && previewContext.length > 0);

  if (!hasAnything) {
    return null;
  }

  return (
    <div className="garmr-panel">
      <div className="garmr-panel-header">
        <div className="garmr-panel-title">
          <span className="garmr-badge">GARMR</span>
          <span>Multi-Provider Orchestration</span>
          {isProcessing && <span className="garmr-processing">routing...</span>}
        </div>
        <button type="button" className="garmr-close" onClick={onClose}>
          ×
        </button>
      </div>

      {/* Context visibility: show what was last sent (or what is queued). */}
      <GarmrContextSection
        label={isProcessing ? "Context being sent" : "Context last sent"}
        context={lastContextSent ?? previewContext ?? null}
      />

      <div className="garmr-panel-messages">
        {messages.map((msg) => (
          <div key={msg.id} className={`garmr-message garmr-message-${msg.role}`}>
            {msg.role === "user" ? (
              <div className="garmr-user-text">{msg.text}</div>
            ) : (
              <Markdown
                value={msg.text}
                className="garmr-assistant-text"
                codeBlockStyle="message"
              />
            )}
          </div>
        ))}
        <div ref={bottomRef} />
      </div>
    </div>
  );
}

type GarmrContextSectionProps = {
  label: string;
  context: OrchestrationContextMsg[] | null;
};

/** Collapsible preview of the conversation context that the orchestrator sees. */
function GarmrContextSection({ label, context }: GarmrContextSectionProps) {
  const [expanded, setExpanded] = useState(false);
  const tokens = useMemo(() => estimateContextTokens(context), [context]);
  if (!context || context.length === 0) {
    return (
      <div className="garmr-context-section garmr-context-section-empty">
        <span className="garmr-context-label">{label}: none</span>
      </div>
    );
  }
  return (
    <div className="garmr-context-section">
      <button
        type="button"
        className="garmr-context-toggle"
        onClick={() => setExpanded((prev) => !prev)}
        aria-expanded={expanded}
      >
        <span className="garmr-context-chevron" aria-hidden>
          {expanded ? "▾" : "▸"}
        </span>
        <span className="garmr-context-label">{label}:</span>
        <span className="garmr-context-meta">
          {context.length} {context.length === 1 ? "message" : "messages"}
          {" · ~"}
          {tokens.toLocaleString()} tokens
        </span>
      </button>
      {expanded && (
        <ol className="garmr-context-transcript">
          {context.map((msg, idx) => (
            <li
              key={`${idx}-${msg.role}`}
              className={`garmr-context-msg garmr-context-msg-${msg.role}`}
            >
              <span className="garmr-context-role">{msg.role}</span>
              <Markdown
                value={msg.text}
                className="garmr-context-text"
                codeBlockStyle="message"
              />
            </li>
          ))}
        </ol>
      )}
    </div>
  );
}
