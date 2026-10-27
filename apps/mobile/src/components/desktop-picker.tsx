import { useCallback, useEffect, useState } from "react";
import { ActivityIndicator, Pressable, TextInput, View } from "react-native";
import { ChevronDown, Monitor, RefreshCw } from "lucide-react-native";
import { DEFAULT_PORT, isIPv4, isValidPairingCode, PAIRING_CODE_LENGTH, type ConnectTarget } from "@celmic/protocol";

import type { DiscoveredDesktop } from "../../modules/celmic-audio";
import { Button } from "@/components/ui/button";
import { Text } from "@/components/ui/text";
import { useColor } from "@/lib/on-air";
import { discover } from "@/lib/stream";
import { cn } from "@/lib/utils";

type Props = { onConnect: (target: ConnectTarget) => void };

const inputClass = "h-12 rounded-md border border-input bg-muted px-4 font-mono-medium text-lg text-foreground";

/** Computers found via DNS-SD, listed like a J-card's tracks; picking one asks for the code. */
export function DesktopPicker({ onConnect }: Props) {
  const [desktops, setDesktops] = useState<DiscoveredDesktop[]>([]);
  const [scanning, setScanning] = useState(false);
  const [selected, setSelected] = useState<string | null>(null);
  const [manual, setManual] = useState(false);
  const muted = useColor("muted-foreground");
  const foreground = useColor("foreground");

  const refresh = useCallback(async () => {
    setScanning(true);
    try {
      setDesktops(await discover());
    } catch {
      setDesktops([]);
    } finally {
      setScanning(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  return (
    <View className="gap-3">
      <View className="flex-row items-center justify-between">
        <Text variant="label">On this network</Text>
        <Pressable
          onPress={refresh}
          disabled={scanning}
          hitSlop={12}
          accessibilityRole="button"
          accessibilityLabel="Look for computers again"
          className="size-10 items-center justify-center rounded-md active:bg-accent"
        >
          {scanning ? <ActivityIndicator color={muted} /> : <RefreshCw size={20} color={muted} />}
        </Pressable>
      </View>

      <View className="border-t border-border">
        {desktops.length === 0 && !scanning && (
          <Text className="border-b border-border py-4 text-muted-foreground">
            No computer found. Open celmic on the PC and keep both on the same Wi-Fi.
          </Text>
        )}
        {desktops.map((desktop, i) => {
          const open = selected === desktop.name;
          return (
            <View key={desktop.name} className="border-b border-border">
              <Pressable
                onPress={() => setSelected(open ? null : desktop.name)}
                accessibilityRole="button"
                accessibilityState={{ expanded: open }}
                className="flex-row items-center gap-4 py-4 active:opacity-70"
              >
                <Text variant="figure" className="w-7 font-mono-medium text-sm text-faint">
                  B{i + 1}
                </Text>
                <Monitor size={22} color={foreground} strokeWidth={1.75} />
                <View className="flex-1">
                  <Text className="font-sans-semibold text-lg">{desktop.name}</Text>
                  <Text variant="figure" className="font-mono-medium text-sm text-muted-foreground">
                    {desktop.hosts[0]}
                  </Text>
                </View>
                <ChevronDown
                  size={20}
                  color={muted}
                  style={{ transform: [{ rotate: open ? "180deg" : "0deg" }] }}
                />
              </Pressable>
              {open && (
                <View className="gap-2 pb-4">
                  <Text variant="muted">Type the 6-digit code shown on {desktop.name}.</Text>
                  <CodeInput
                    onComplete={(code) => onConnect({ hosts: desktop.hosts, port: desktop.port, code, name: desktop.name })}
                  />
                </View>
              )}
            </View>
          );
        })}
      </View>

      <Pressable onPress={() => setManual((m) => !m)} hitSlop={8} className="self-start py-1">
        <Text className="text-muted-foreground underline">{manual ? "Hide manual connection" : "Enter the address manually"}</Text>
      </Pressable>
      {manual && <ManualForm onConnect={onConnect} />}
    </View>
  );
}

function CodeInput({ onComplete }: { onComplete: (code: string) => void }) {
  const [code, setCode] = useState("");
  const placeholder = useColor("faint");
  return (
    <TextInput
      autoFocus
      value={code}
      onChangeText={(text) => {
        const digits = text.replace(/\D/g, "").slice(0, PAIRING_CODE_LENGTH);
        setCode(digits);
        if (isValidPairingCode(digits)) onComplete(digits);
      }}
      keyboardType="number-pad"
      placeholder="000000"
      placeholderTextColor={placeholder}
      maxLength={PAIRING_CODE_LENGTH}
      accessibilityLabel="Pairing code"
      className={cn(inputClass, "h-16 text-center font-mono text-3xl tracking-[0.4em]")}
    />
  );
}

function ManualForm({ onConnect }: Props) {
  const [host, setHost] = useState("");
  const [port, setPort] = useState(String(DEFAULT_PORT));
  const [code, setCode] = useState("");
  const placeholder = useColor("faint");
  const portNumber = Number(port);
  const valid =
    isIPv4(host.trim()) && /^\d+$/.test(port) && portNumber >= 1 && portNumber <= 65535 && isValidPairingCode(code);

  return (
    <View className="gap-3">
      <TextInput
        value={host}
        onChangeText={setHost}
        placeholder="192.168.0.10"
        placeholderTextColor={placeholder}
        keyboardType="decimal-pad"
        autoCorrect={false}
        accessibilityLabel="Computer address"
        className={inputClass}
      />
      <View className="flex-row gap-3">
        <TextInput
          value={port}
          onChangeText={(t) => setPort(t.replace(/\D/g, "").slice(0, 5))}
          keyboardType="number-pad"
          accessibilityLabel="Port"
          className={cn(inputClass, "flex-1")}
        />
        <TextInput
          value={code}
          onChangeText={(t) => setCode(t.replace(/\D/g, "").slice(0, PAIRING_CODE_LENGTH))}
          placeholder="Code"
          placeholderTextColor={placeholder}
          keyboardType="number-pad"
          accessibilityLabel="Pairing code"
          className={cn(inputClass, "flex-[1.4]")}
        />
      </View>
      <Button disabled={!valid} onPress={() => onConnect({ hosts: [host.trim()], port: portNumber, code })}>
        <Text>Connect</Text>
      </Button>
    </View>
  );
}
