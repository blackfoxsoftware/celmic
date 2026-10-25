import { KeyboardAvoidingView, Platform, Pressable, ScrollView, StyleSheet, Switch, Text, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { router } from "expo-router";

import { DesktopPicker } from "@/components/desktop-picker";
import { MicOrb } from "@/components/mic-orb";
import {
  connect,
  disconnect,
  dismissError,
  isActive,
  setVoiceProcessing,
  toggleMute,
  useStream,
} from "@/lib/stream";
import { colors, mono } from "@/lib/theme";

const STATE_LABEL = {
  idle: "Not connected",
  connecting: "Connecting…",
  streaming: "Live",
  reconnecting: "Reconnecting…",
  error: "Disconnected",
} as const;

export default function Home() {
  const { stream, stats, target, muted, voiceProcessing } = useStream();
  const active = isActive(stream.state);
  const live = stream.state === "streaming";
  const desktop = stream.desktopName ?? target?.name;


  return (
    <SafeAreaView style={styles.safe}>
      <KeyboardAvoidingView behavior={Platform.OS === "ios" ? "padding" : undefined} style={{ flex: 1 }}>
        <ScrollView contentContainerStyle={styles.content} keyboardShouldPersistTaps="handled">
          <View style={styles.header}>
            <Text style={styles.brand}>celmic</Text>
            <View style={[styles.pill, live && styles.pillLive]}>
              <View style={[styles.pillDot, { backgroundColor: live ? colors.accent : active ? colors.warn : colors.muted }]} />
              <Text style={styles.pillText}>{STATE_LABEL[stream.state]}</Text>
            </View>
          </View>

          <MicOrb live={active} muted={muted} level={stats?.level ?? 0} onPress={toggleMute} />

          {active ? (
            <View style={styles.card}>
              <Text style={styles.cardTitle}>{desktop ? `Streaming to ${desktop}` : "Looking for your computer…"}</Text>
              <View style={styles.statsRow}>
                <Stat label="Latency" value={stats && stats.rttMs >= 0 ? `${stats.rttMs.toFixed(0)} ms` : "—"} />
                <Stat label="Rate" value={stats ? `${stats.sampleRate / 1000} kHz` : "—"} />
                <Stat label="Packets" value={stats ? stats.packetsSent.toLocaleString() : "—"} />
              </View>
              <Pressable style={styles.stopButton} onPress={disconnect}>
                <Text style={styles.stopText}>Disconnect</Text>
              </Pressable>
            </View>
          ) : (
            <>
              {stream.state === "error" && (
                <Pressable style={styles.error} onPress={dismissError}>
                  <Text style={styles.errorText}>{stream.error ?? "Something went wrong."}</Text>
                  {target && stream.reason !== "bad_code" && (
                    <Pressable onPress={() => connect(target)} hitSlop={8}>
                      <Text style={styles.retry}>Try again</Text>
                    </Pressable>
                  )}
                </Pressable>
              )}

              <Pressable style={styles.primary} onPress={() => router.push("/scan")}>
                <Text style={styles.primaryText}>Scan QR code</Text>
              </Pressable>

              <DesktopPicker onConnect={connect} />

              <View style={styles.setting}>
                <View style={{ flex: 1 }}>
                  <Text style={styles.settingTitle}>Voice processing</Text>
                  <Text style={styles.settingSub}>
                    Noise suppression and echo cancellation. Turn off for music or a raw signal.
                  </Text>
                </View>
                <Switch
                  value={voiceProcessing}
                  onValueChange={setVoiceProcessing}
                  trackColor={{ true: colors.accent, false: colors.surface2 }}
                  thumbColor={Platform.OS === "android" ? colors.text : undefined}
                />
              </View>
            </>
          )}
        </ScrollView>
      </KeyboardAvoidingView>
    </SafeAreaView>
  );
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <View style={{ flex: 1 }}>
      <Text style={styles.statLabel}>{label}</Text>
      <Text style={styles.statValue}>{value}</Text>
    </View>
  );
}

const styles = StyleSheet.create({
  safe: { flex: 1, backgroundColor: colors.bg },
  content: { padding: 20, gap: 16, paddingBottom: 48 },
  header: { flexDirection: "row", alignItems: "center", justifyContent: "space-between" },
  brand: { color: colors.text, fontSize: 24, fontWeight: "800", letterSpacing: -0.5 },
  pill: {
    flexDirection: "row",
    alignItems: "center",
    gap: 8,
    paddingHorizontal: 12,
    paddingVertical: 6,
    borderRadius: 999,
    borderWidth: 1,
    borderColor: colors.border,
    backgroundColor: colors.surface,
  },
  pillLive: { borderColor: colors.accent },
  pillDot: { width: 8, height: 8, borderRadius: 4 },
  pillText: { color: colors.text, fontSize: 13, fontWeight: "600" },
  card: {
    gap: 16,
    padding: 16,
    borderRadius: 16,
    borderWidth: 1,
    borderColor: colors.border,
    backgroundColor: colors.surface,
  },
  cardTitle: { color: colors.text, fontSize: 17, fontWeight: "700" },
  statsRow: { flexDirection: "row", gap: 12 },
  statLabel: { color: colors.muted, fontSize: 12 },
  statValue: { color: colors.text, fontSize: 16, fontWeight: "600", fontFamily: mono, marginTop: 2 },
  stopButton: {
    borderRadius: 12,
    paddingVertical: 14,
    alignItems: "center",
    borderWidth: 1,
    borderColor: colors.border,
  },
  stopText: { color: colors.text, fontSize: 16, fontWeight: "600" },
  primary: { backgroundColor: colors.accent, borderRadius: 14, paddingVertical: 16, alignItems: "center" },
  primaryText: { color: colors.accentInk, fontWeight: "800", fontSize: 17 },
  error: {
    gap: 8,
    padding: 14,
    borderRadius: 12,
    backgroundColor: "#2A1416",
    borderWidth: 1,
    borderColor: "#4A1F22",
  },
  errorText: { color: colors.danger, fontSize: 15 },
  retry: { color: colors.text, fontWeight: "700", fontSize: 15 },
  setting: {
    flexDirection: "row",
    alignItems: "center",
    gap: 16,
    padding: 16,
    borderRadius: 16,
    borderWidth: 1,
    borderColor: colors.border,
    backgroundColor: colors.surface,
  },
  settingTitle: { color: colors.text, fontSize: 16, fontWeight: "600" },
  settingSub: { color: colors.muted, fontSize: 13, marginTop: 2, lineHeight: 18 },
});
