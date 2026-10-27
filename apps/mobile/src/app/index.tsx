import { useEffect, useState } from "react";
import { KeyboardAvoidingView, Platform, ScrollView, View } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { router } from "expo-router";
import { ScanLine } from "lucide-react-native";

import { DesktopPicker } from "@/components/desktop-picker";
import { IndexStripe } from "@/components/index-stripe";
import { MuteKey } from "@/components/mute-key";
import { TapeCounter, formatCounter } from "@/components/tape-counter";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { Text } from "@/components/ui/text";
import { onAir, useColor } from "@/lib/on-air";
import { connect, disconnect, isActive, setVoiceProcessing, toggleMute, useStream } from "@/lib/stream";

export default function Home() {
  const { stream, stats, target, muted, voiceProcessing, liveSince } = useStream();
  const insets = useSafeAreaInsets();
  const active = isActive(stream.state);
  const state = onAir(stream, muted, stream.desktopName ?? target?.name);
  const onPrimary = useColor("primary-foreground");

  return (
    <View className="flex-1 bg-background">
      <IndexStripe
        state={state}
        topInset={insets.top}
        counter={active && liveSince ? <OnAirCounter since={liveSince} /> : undefined}
      />

      {active ? (
        <View className="flex-1 gap-5 px-5 pt-5" style={{ paddingBottom: insets.bottom + 16 }}>
          <MuteKey muted={muted} level={stats?.level ?? 0} onToggle={toggleMute} />
          <View className="border-t border-border">
            <Track side="A1" label="Round trip" value={stats && stats.rttMs >= 0 ? `${stats.rttMs.toFixed(0)} ms` : "…"} />
            <Track side="A2" label="Sample rate" value={stats ? `${stats.sampleRate / 1000} kHz` : "…"} />
            <Track side="A3" label="Packets sent" value={stats ? stats.packetsSent.toLocaleString() : "…"} />
          </View>
          <Button variant="outline" onPress={disconnect}>
            <Text>Disconnect</Text>
          </Button>
        </View>
      ) : (
        <KeyboardAvoidingView behavior={Platform.OS === "ios" ? "padding" : undefined} className="flex-1">
          <ScrollView keyboardShouldPersistTaps="handled">
            <View className="gap-8 px-5 pt-6" style={{ paddingBottom: insets.bottom + 32 }}>
              <View className="gap-3">
                <Button size="lg" onPress={() => router.push("/scan")}>
                  <ScanLine size={22} color={onPrimary} />
                  <Text>Scan the code on the PC</Text>
                </Button>
                {stream.state === "error" && target && stream.reason !== "bad_code" && (
                  <Button variant="outline" onPress={() => connect(target)}>
                    <Text>Try {target.name ?? "again"}</Text>
                  </Button>
                )}
              </View>

              <DesktopPicker onConnect={connect} />

              <View className="flex-row items-center gap-4 border-t border-border pt-6">
                <View className="flex-1 gap-1">
                  <Text className="font-sans-semibold text-lg">Voice processing</Text>
                  <Text variant="muted" className="leading-5">
                    Noise suppression, echo cancellation and gain control. Turn off for music or a raw signal.
                  </Text>
                </View>
                <Switch
                  checked={voiceProcessing}
                  onCheckedChange={setVoiceProcessing}
                  accessibilityLabel="Voice processing"
                />
              </View>
            </View>
          </ScrollView>
        </KeyboardAvoidingView>
      )}
    </View>
  );
}

function OnAirCounter({ since }: { since: number }) {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, []);
  const secs = Math.max(0, Math.floor((now - since) / 1000));
  return <TapeCounter value={formatCounter(secs)} size={22} label={`${secs} seconds on air`} separatorClassName="text-background" />;
}

function Track({ side, label, value }: { side: string; label: string; value: string }) {
  return (
    <View className="flex-row items-baseline gap-3 border-b border-border py-3">
      <Text className="w-7 font-mono-medium text-[13px] text-faint">{side}</Text>
      <Text className="flex-1 font-sans-medium text-muted-foreground">{label}</Text>
      <Text className="font-mono text-base">{value}</Text>
    </View>
  );
}
