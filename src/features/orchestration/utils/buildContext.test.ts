import { describe, expect, it } from "vitest";
import type { ConversationItem } from "@/types";
import {
  buildContextFromItems,
  estimateContextTokens,
  DEFAULT_CONTEXT_LIMIT,
} from "./buildContext";

const message = (
  id: string,
  role: "user" | "assistant",
  text: string,
): ConversationItem => ({ id, kind: "message", role, text });

const reasoning = (id: string): ConversationItem => ({
  id,
  kind: "reasoning",
  summary: "rsum",
  content: "rbody",
});

const diff = (id: string): ConversationItem => ({
  id,
  kind: "diff",
  title: "t",
  diff: "-a\n+b",
});

describe("buildContextFromItems", () => {
  it("returns null for null / undefined / empty input", () => {
    expect(buildContextFromItems(null)).toBeNull();
    expect(buildContextFromItems(undefined)).toBeNull();
    expect(buildContextFromItems([])).toBeNull();
  });

  it("returns null when limit is 0 or negative", () => {
    expect(buildContextFromItems([message("1", "user", "hi")], 0)).toBeNull();
    expect(buildContextFromItems([message("1", "user", "hi")], -5)).toBeNull();
  });

  it("keeps only message items, dropping reasoning/diff/etc.", () => {
    const items: ConversationItem[] = [
      message("1", "user", "ask"),
      reasoning("2"),
      diff("3"),
      message("4", "assistant", "reply"),
    ];
    const ctx = buildContextFromItems(items);
    expect(ctx).toEqual([
      { role: "user", text: "ask" },
      { role: "assistant", text: "reply" },
    ]);
  });

  it("strips empty message bodies", () => {
    const items: ConversationItem[] = [
      message("1", "user", "real"),
      message("2", "assistant", ""),
      message("3", "assistant", "   "),
      message("4", "user", "also real"),
    ];
    const ctx = buildContextFromItems(items);
    expect(ctx?.map((m) => m.text)).toEqual(["real", "also real"]);
  });

  it("returns only the last `limit` messages (most recent)", () => {
    const items: ConversationItem[] = [];
    for (let i = 1; i <= 25; i++) {
      items.push(message(`${i}`, i % 2 ? "user" : "assistant", `msg ${i}`));
    }
    const ctx = buildContextFromItems(items, 5);
    expect(ctx).toHaveLength(5);
    expect(ctx?.[0].text).toBe("msg 21");
    expect(ctx?.[4].text).toBe("msg 25");
  });

  it("defaults to DEFAULT_CONTEXT_LIMIT when no limit given", () => {
    const items: ConversationItem[] = [];
    for (let i = 1; i <= DEFAULT_CONTEXT_LIMIT + 5; i++) {
      items.push(message(`${i}`, "user", `msg ${i}`));
    }
    const ctx = buildContextFromItems(items);
    expect(ctx).toHaveLength(DEFAULT_CONTEXT_LIMIT);
  });

  it("truncates long message bodies", () => {
    const long = "x".repeat(5000);
    const items: ConversationItem[] = [message("1", "user", long)];
    const ctx = buildContextFromItems(items);
    expect(ctx).toHaveLength(1);
    expect(ctx?.[0].text.endsWith("[truncated]")).toBe(true);
    expect(ctx?.[0].text.length).toBeLessThan(long.length);
  });

  it("returns null if input has only non-message items", () => {
    const items: ConversationItem[] = [reasoning("1"), diff("2")];
    expect(buildContextFromItems(items)).toBeNull();
  });
});

describe("estimateContextTokens", () => {
  it("returns 0 for null / empty", () => {
    expect(estimateContextTokens(null)).toBe(0);
    expect(estimateContextTokens([])).toBe(0);
  });

  it("approximates 4 chars per token", () => {
    const ctx = [{ role: "user" as const, text: "x".repeat(40) }];
    // 40 chars + role label + separators ≈ 48 → ~12 tokens
    const tokens = estimateContextTokens(ctx);
    expect(tokens).toBeGreaterThan(10);
    expect(tokens).toBeLessThan(15);
  });
});
