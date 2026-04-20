import { useCallback, useEffect, useState } from "react";
import type {
  AppSettings,
  OrchestrationSettings,
  OrchestrationTier,
  OrchestrationProvider,
  ProviderAvailability,
} from "@/types";
import {
  SettingsSection,
  SettingsToggleRow,
  SettingsToggleSwitch,
} from "@/features/design-system/components/settings/SettingsPrimitives";
import { checkProviderAvailability } from "@/services/tauri";

type SettingsOrchestrationSectionProps = {
  appSettings: AppSettings;
  onUpdateAppSettings: (next: AppSettings) => Promise<void>;
};

const TIER_OPTIONS: { value: OrchestrationTier; label: string; desc: string }[] = [
  { value: "fast", label: "Fast", desc: "Single model, low latency" },
  { value: "medium", label: "Medium", desc: "Executor + reviewer" },
  { value: "heavy", label: "Heavy", desc: "Full multi-agent council" },
];

const PROVIDER_LABELS: Record<OrchestrationProvider, string> = {
  claude: "Claude",
  gemini: "Gemini",
  codex: "Codex (OpenAI)",
};

export function SettingsOrchestrationSection({
  appSettings,
  onUpdateAppSettings,
}: SettingsOrchestrationSectionProps) {
  const orch = appSettings.orchestration;
  const [availability, setAvailability] = useState<ProviderAvailability | null>(null);
  const [checkingAvailability, setCheckingAvailability] = useState(false);

  const update = useCallback(
    (patch: Partial<OrchestrationSettings>) => {
      const next: AppSettings = {
        ...appSettings,
        orchestration: { ...orch, ...patch },
      };
      return onUpdateAppSettings(next);
    },
    [appSettings, orch, onUpdateAppSettings],
  );

  const refreshAvailability = useCallback(async () => {
    setCheckingAvailability(true);
    try {
      const result = await checkProviderAvailability();
      setAvailability(result);
    } catch {
      // ignore
    } finally {
      setCheckingAvailability(false);
    }
  }, []);

  useEffect(() => {
    refreshAvailability();
  }, [refreshAvailability]);

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
      <SettingsSection
        title="Multi-Provider Orchestration"
        subtitle="Route tasks through multiple AI providers working in unison. When enabled, queries are classified by complexity and routed to the optimal tier."
      >
        <SettingsToggleRow
          title="Enable orchestration"
          subtitle="Activate multi-provider task routing"
        >
          <SettingsToggleSwitch
            pressed={orch.enabled}
            onClick={() => void update({ enabled: !orch.enabled })}
          />
        </SettingsToggleRow>
      </SettingsSection>

      {orch.enabled && (
        <>
          <SettingsSection title="Tier Routing">
            <SettingsToggleRow
              title="Auto-classify tier"
              subtitle="Automatically determine task complexity and route to the appropriate tier"
            >
              <SettingsToggleSwitch
                pressed={orch.autoTier}
                onClick={() => void update({ autoTier: !orch.autoTier })}
              />
            </SettingsToggleRow>

            {!orch.autoTier && (
              <div style={{ marginTop: 8 }}>
                <label style={{ fontSize: 13, color: "var(--text-secondary)", display: "block", marginBottom: 4 }}>
                  Default tier
                </label>
                <div style={{ display: "flex", gap: 8 }}>
                  {TIER_OPTIONS.map((opt) => (
                    <button
                      key={opt.value}
                      type="button"
                      onClick={() => void update({ defaultTier: opt.value })}
                      style={{
                        flex: 1,
                        padding: "8px 12px",
                        borderRadius: 8,
                        border: orch.defaultTier === opt.value
                          ? "2px solid var(--accent)"
                          : "1px solid var(--border)",
                        background: orch.defaultTier === opt.value
                          ? "var(--accent-bg)"
                          : "transparent",
                        cursor: "pointer",
                        textAlign: "left",
                        fontSize: 13,
                      }}
                    >
                      <div style={{ fontWeight: 600 }}>{opt.label}</div>
                      <div style={{ fontSize: 11, color: "var(--text-secondary)", marginTop: 2 }}>
                        {opt.desc}
                      </div>
                    </button>
                  ))}
                </div>
              </div>
            )}
          </SettingsSection>

          <SettingsSection title="Providers">
            <div style={{ fontSize: 13, color: "var(--text-secondary)", marginBottom: 8 }}>
              Enable the CLI providers available on your system.
              {availability && (
                <span style={{ marginLeft: 4 }}>
                  Detected: {[
                    availability.claude && "Claude",
                    availability.gemini && "Gemini",
                    availability.codex && "Codex",
                  ].filter(Boolean).join(", ") || "none"}
                </span>
              )}
              <button
                type="button"
                onClick={refreshAvailability}
                disabled={checkingAvailability}
                style={{
                  marginLeft: 8,
                  fontSize: 12,
                  color: "var(--accent)",
                  background: "none",
                  border: "none",
                  cursor: "pointer",
                  textDecoration: "underline",
                }}
              >
                {checkingAvailability ? "checking..." : "refresh"}
              </button>
            </div>

            {(["claude", "gemini", "codex"] as OrchestrationProvider[]).map((providerId) => {
              const providerConfig = orch.providers[providerId];
              const isAvailable = availability?.[providerId] ?? null;
              return (
                <SettingsToggleRow
                  key={providerId}
                  title={
                    <span style={{ display: "flex", alignItems: "center", gap: 8 }}>
                      {PROVIDER_LABELS[providerId]}
                      {isAvailable !== null && (
                        <span
                          style={{
                            fontSize: 11,
                            padding: "1px 6px",
                            borderRadius: 4,
                            background: isAvailable ? "var(--success-bg)" : "var(--error-bg)",
                            color: isAvailable ? "var(--success)" : "var(--error)",
                          }}
                        >
                          {isAvailable ? "installed" : "not found"}
                        </span>
                      )}
                    </span>
                  }
                  subtitle={providerConfig.defaultModel ? `Model: ${providerConfig.defaultModel}` : undefined}
                >
                  <SettingsToggleSwitch
                    pressed={providerConfig.enabled}
                    onClick={() => {
                      const next: AppSettings = {
                        ...appSettings,
                        orchestration: {
                          ...orch,
                          providers: {
                            ...orch.providers,
                            [providerId]: { ...providerConfig, enabled: !providerConfig.enabled },
                          },
                        },
                      };
                      void onUpdateAppSettings(next);
                    }}
                  />
                </SettingsToggleRow>
              );
            })}
          </SettingsSection>

          <SettingsSection title="Tier Configuration">
            {TIER_OPTIONS.map((tier) => {
              const tierKey = tier.value;
              const config = orch.tierConfig[tierKey];
              return (
                <div
                  key={tierKey}
                  style={{
                    padding: "8px 0",
                    borderBottom: "1px solid var(--border)",
                  }}
                >
                  <div style={{ fontWeight: 600, fontSize: 13, marginBottom: 4 }}>
                    {tier.label} — {tier.desc}
                  </div>
                  <div style={{ display: "flex", gap: 16, fontSize: 12 }}>
                    <div>
                      <span style={{ color: "var(--text-secondary)" }}>Executor: </span>
                      <span>{PROVIDER_LABELS[config.executor]}</span>
                      {config.executorModel && (
                        <span style={{ color: "var(--text-tertiary)" }}> ({config.executorModel})</span>
                      )}
                    </div>
                    {config.reviewer && (
                      <div>
                        <span style={{ color: "var(--text-secondary)" }}>Reviewer: </span>
                        <span>{PROVIDER_LABELS[config.reviewer]}</span>
                        {config.reviewerModel && (
                          <span style={{ color: "var(--text-tertiary)" }}> ({config.reviewerModel})</span>
                        )}
                      </div>
                    )}
                    <div>
                      <span style={{ color: "var(--text-secondary)" }}>Timeout: </span>
                      <span>{(config.timeoutMs / 1000).toFixed(0)}s</span>
                    </div>
                  </div>
                </div>
              );
            })}
          </SettingsSection>
        </>
      )}
    </div>
  );
}
