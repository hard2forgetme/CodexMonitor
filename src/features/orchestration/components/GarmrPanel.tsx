import { useEffect, useRef } from "react";

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
};

export function GarmrPanel({ messages, isProcessing, onClose }: GarmrPanelProps) {
  const bottomRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages]);

  if (messages.length === 0 && !isProcessing) {
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
      <div className="garmr-panel-messages">
        {messages.map((msg) => (
          <div key={msg.id} className={`garmr-message garmr-message-${msg.role}`}>
            {msg.role === "user" ? (
              <div className="garmr-user-text">{msg.text}</div>
            ) : (
              <div
                className="garmr-assistant-text"
                // eslint-disable-next-line react/no-danger
                dangerouslySetInnerHTML={{ __html: renderMarkdownSimple(msg.text) }}
              />
            )}
          </div>
        ))}
        <div ref={bottomRef} />
      </div>
    </div>
  );
}

function renderMarkdownSimple(text: string): string {
  // 1. Escape HTML entities first (XSS prevention).
  let result = text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");

  // 2. Extract code blocks BEFORE inline processing to prevent conflicts.
  const codeBlocks: string[] = [];
  result = result.replace(/```(\w*)\n([\s\S]*?)```/g, (_match, _lang, code) => {
    const idx = codeBlocks.length;
    codeBlocks.push(`<pre><code>${code}</code></pre>`);
    return `\x00CODEBLOCK${idx}\x00`;
  });

  // 3. Inline formatting (safe now that code blocks are extracted).
  result = result
    .replace(/\*\*(.+?)\*\*/g, "<strong>$1</strong>")
    .replace(/`([^`]+)`/g, "<code>$1</code>")
    .replace(/^---$/gm, "<hr>")
    .replace(/\n/g, "<br>");

  // 4. Restore code blocks.
  result = result.replace(/\x00CODEBLOCK(\d+)\x00/g, (_match, idx) => codeBlocks[Number(idx)]);

  return result;
}
