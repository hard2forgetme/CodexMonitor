import { useCallback, useState } from "react";
import type {
  AppSettings,
  AppMention,
  ComposerSendIntent,
  ConversationItem,
  OrchestrationResult,
} from "@/types";
import {
  classifyOrchestrationTask,
  runOrchestration,
  runGarmrAgent,
  type OrchestrationContextMsg,
} from "@/services/tauri";
import {
  buildContextFromItems,
  DEFAULT_CONTEXT_LIMIT,
} from "@/features/orchestration/utils/buildContext";

type UseGarmrSendOptions = {
  appSettings: AppSettings;
  activeWorkspaceId: string | null;
  activeWorkspacePath: string | null;
  activeThreadId: string | null;
  injectMessage: (
    workspaceId: string,
    threadId: string,
    item: ConversationItem,
  ) => void;
  originalSend: (
    text: string,
    images: string[],
    appMentions?: AppMention[],
    submitIntent?: ComposerSendIntent,
  ) => Promise<void>;
  /**
   * Returns the current ConversationItems for a thread. Typically wired to
   * `useThreads().getItemsForThread`. Used to build the conversation context
   * that we forward to the orchestration pipeline.
   */
  getItemsForThread?: (threadId: string) => ConversationItem[];
  /** Maximum number of recent messages to include as context. */
  contextMessageLimit?: number;
};

/**
 * Format a full orchestration result for display in the thread.
 * Used across all tiers: FAST (Gemini Flash), MEDIUM/HEAVY (GARMR agent pipeline).
 */
function formatOrchestrationResponse(result: OrchestrationResult): string {
  const parts: string[] = [];

  const cls = result.classification;
  const tierLabel = cls.tier.toUpperCase();
  const provider = result.primaryResponse?.provider ?? "unknown";
  const model = result.primaryResponse?.modelUsed ?? "unknown";
  parts.push(
    `**\`GARMR ${tierLabel}\`** · ${provider}/${model} · ` +
    `confidence ${(result.confidence * 100).toFixed(0)}% · ` +
    `${(result.totalDurationMs / 1000).toFixed(1)}s`,
  );
  parts.push("");

  if (result.primaryResponse) {
    const pr = result.primaryResponse;
    if (pr.success) {
      parts.push(pr.output);
    } else {
      parts.push(`**Error** (${pr.provider}/${pr.modelUsed}): ${pr.error ?? pr.output}`);
    }
  }

  if (result.review) {
    const rv = result.review;
    parts.push("");
    parts.push("---");
    parts.push(
      `**Review** by ${rv.reviewer}/${rv.reviewerModel}: ` +
      `${rv.status === "approved" ? "Approved" : "Needs revision"}`,
    );
    if (rv.suggestions) {
      parts.push(rv.suggestions);
    }
  }

  if (result.council) {
    const cn = result.council;
    parts.push("");
    parts.push("---");
    parts.push(`**SYNAPSE Council** (${cn.members.length} members, ${cn.rounds.length} rounds)`);
    parts.push(cn.synthesis);
  }

  return parts.join("\n");
}

export function useGarmrSend({
  appSettings,
  activeWorkspaceId,
  activeWorkspacePath,
  activeThreadId,
  injectMessage,
  originalSend,
  getItemsForThread,
  contextMessageLimit = DEFAULT_CONTEXT_LIMIT,
}: UseGarmrSendOptions) {
  const [isProcessing, setIsProcessing] = useState(false);
  const [lastContextSent, setLastContextSent] = useState<
    OrchestrationContextMsg[] | null
  >(null);

  const isEnabled = appSettings.orchestration?.enabled ?? false;
  const autoTier = appSettings.orchestration?.autoTier ?? true;

  /**
   * Build a preview of the context that WOULD be sent for the current
   * thread. Used by UI (GarmrPanel) to show the transcript before send.
   */
  const previewContext = useCallback((): OrchestrationContextMsg[] | null => {
    if (!activeThreadId || !getItemsForThread) return null;
    const items = getItemsForThread(activeThreadId);
    return buildContextFromItems(items, contextMessageLimit);
  }, [activeThreadId, getItemsForThread, contextMessageLimit]);

  const garmrSend = useCallback(
    async (
      text: string,
      images: string[],
      appMentions?: AppMention[],
      submitIntent?: ComposerSendIntent,
    ) => {
      if (!isEnabled || !activeWorkspaceId || !activeThreadId) {
        return originalSend(text, images, appMentions, submitIntent);
      }

      setIsProcessing(true);

      // Build conversation context BEFORE injecting the new user message,
      // so the context reflects the prior thread (not the just-sent turn).
      const context = previewContext();
      setLastContextSent(context);

      try {
        // Step 1: Classify the task to determine the tier
        const classification = await classifyOrchestrationTask(text);
        const tier = classification.tier;

        // ---------------------------------------------------------------
        // FAST tier: Simple Q&A — run orchestration for a direct answer,
        // inject the response, and skip the Codex agent entirely.
        // ---------------------------------------------------------------
        if (tier === "fast" && autoTier) {
          // Show user message in thread
          injectMessage(activeWorkspaceId, activeThreadId, {
            id: `garmr-user-${Date.now()}`,
            kind: "message",
            role: "user",
            text,
            images: images.length > 0 ? images : undefined,
          });

          const placeholderId = `garmr-asst-${Date.now()}`;
          injectMessage(activeWorkspaceId, activeThreadId, {
            id: placeholderId,
            kind: "message",
            role: "assistant",
            text: "**`GARMR FAST`** Routing to Gemini Flash...",
          });

          try {
            const result = await runOrchestration({
              query: text,
              cwd: activeWorkspacePath,
              tierOverride: "fast",
              context,
            });

            injectMessage(activeWorkspaceId, activeThreadId, {
              id: placeholderId,
              kind: "message",
              role: "assistant",
              text: formatOrchestrationResponse(result),
            });
          } catch (err) {
            injectMessage(activeWorkspaceId, activeThreadId, {
              id: placeholderId,
              kind: "message",
              role: "assistant",
              text: `**\`GARMR Error\`** ${err instanceof Error ? err.message : String(err)}`,
            });
          }

          setIsProcessing(false);
          return;
        }

        // ---------------------------------------------------------------
        // MEDIUM / HEAVY tier: Full GARMR agent pipeline.
        // Gemini plans → Claude executes in agent mode → Gemini reviews.
        // This bypasses Codex entirely — Claude handles all heavy coding.
        // ---------------------------------------------------------------

        // Show user message in thread
        injectMessage(activeWorkspaceId, activeThreadId, {
          id: `garmr-user-${Date.now()}`,
          kind: "message",
          role: "user",
          text,
          images: images.length > 0 ? images : undefined,
        });

        // Inject a status message so the user sees GARMR is working
        const statusId = `garmr-status-${Date.now()}`;
        injectMessage(activeWorkspaceId, activeThreadId, {
          id: statusId,
          kind: "message",
          role: "assistant",
          text: `**\`GARMR ${tier.toUpperCase()}\`** ${tier === "heavy" ? "SYNAPSE council planning → Claude agent executing" : "Gemini planning → Claude agent executing → Gemini reviewing"}...`,
        });

        try {
          // Run the full GARMR agent pipeline:
          // 1. Gemini creates the plan
          // 2. Claude executes in full agent mode with the plan
          // 3. Gemini reviews (MEDIUM tier)
          const result: OrchestrationResult = await runGarmrAgent({
            query: text,
            cwd: activeWorkspacePath,
            tier,
            context,
          });

          // Replace the status message with the full response
          injectMessage(activeWorkspaceId, activeThreadId, {
            id: statusId,
            kind: "message",
            role: "assistant",
            text: formatOrchestrationResponse(result),
          });
        } catch (err) {
          // If the GARMR agent pipeline fails, show error and fall back to Codex
          console.warn("[GARMR] Agent pipeline failed, falling back to Codex:", err);
          injectMessage(activeWorkspaceId, activeThreadId, {
            id: statusId,
            kind: "message",
            role: "assistant",
            text: `**\`GARMR Error\`** Agent pipeline failed: ${err instanceof Error ? err.message : String(err)}\n\nFalling back to Codex...`,
          });
          setIsProcessing(false);
          return originalSend(text, images, appMentions, submitIntent);
        }

        setIsProcessing(false);
        return;

      } catch (err) {
        // If orchestration itself fails, fall through to normal Codex send
        console.warn("[GARMR] Orchestration failed, falling back to Codex:", err);
        setIsProcessing(false);
        return originalSend(text, images, appMentions, submitIntent);
      }
    },
    [
      isEnabled,
      autoTier,
      activeWorkspaceId,
      activeWorkspacePath,
      activeThreadId,
      injectMessage,
      originalSend,
      previewContext,
    ],
  );

  return {
    garmrSend,
    isGarmrEnabled: isEnabled,
    isGarmrProcessing: isProcessing,
    /** Preview of the context that WOULD be sent if the user hits send now. */
    previewContext,
    /** The context that was actually sent in the most recent dispatch. */
    lastContextSent,
  };
}
