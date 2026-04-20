import { useEffect, useRef } from "react";
import { Markdown } from "../../messages/components/Markdown";

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
