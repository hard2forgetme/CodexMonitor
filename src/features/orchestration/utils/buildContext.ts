import type { ConversationItem } from "@/types";
import type { OrchestrationContextMsg } from "@/services/tauri";

/** Default number of recent messages to include as context. */
export const DEFAULT_CONTEXT_LIMIT = 15;

/** Max characters per message — keeps token budget predictable. */
const MAX_CHARS_PER_MSG = 2000;

/**
 * Extract the most recent N "message" items from a thread and compact them
 * into OrchestrationContextMsg[] for transport to the Rust orchestrator.
 *
 * Only plain user/assistant messages are included. Reasoning, diff, review,
 * and explore items are intentionally stripped — they're either too bulky
 * (diffs can be thousands of lines) or too noisy for follow-up resolution.
 *
 * Images are dropped: the orchestration pipeline is text-only for now.
 *
 * Returns null when there's nothing usable, which callers pass through as
 * "no context" rather than "empty context".
 */
export function buildContextFromItems(
  items: ConversationItem[] | null | undefined,
  limit: number = DEFAULT_CONTEXT_LIMIT,
): OrchestrationContextMsg[] | null {
  if (!items || items.length === 0 || limit <= 0) {
    return null;
  }

  const messages: OrchestrationContextMsg[] = [];
  for (const item of items) {
    if (item.kind !== "message") {
      continue;
    }
    // Skip empty bodies.
    const text = typeof item.text === "string" ? item.text.trim() : "";
    if (!text) {
      continue;
    }
    messages.push({
      role: item.role,
      text: text.length > MAX_CHARS_PER_MSG
        ? `${text.slice(0, MAX_CHARS_PER_MSG)}… [truncated]`
        : text,
    });
  }

  if (messages.length === 0) {
    return null;
  }

  // Keep only the last `limit` items (most recent).
  return messages.slice(-limit);
}

/**
 * Estimate token count for a built context. Approximates 1 token ≈ 4 chars
 * (OpenAI's common rule of thumb); close enough for UI display.
 */
export function estimateContextTokens(
  context: OrchestrationContextMsg[] | null | undefined,
): number {
  if (!context) return 0;
  let chars = 0;
  for (const msg of context) {
    chars += msg.text.length + msg.role.length + 4; // role label + separators
  }
  return Math.ceil(chars / 4);
}
