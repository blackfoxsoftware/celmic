import { useCallback, useEffect, useState } from "react";
import { ActivityIndicator, Pressable, StyleSheet, Text, TextInput, View } from "react-native";
import { DEFAULT_PORT, isIPv4, isValidPairingCode, PAIRING_CODE_LENGTH, type ConnectTarget } from "@celmic/protocol";

import type { DiscoveredDesktop } from "../../modules/celmic-audio";
import { discover } from "@/lib/stream";
import { colors, mono } from "@/lib/theme";

type Props = { onConnect: (target: ConnectTarget) => void };

/** Lists computers found via DNS-SD; picking one asks for the 6-digit code. */
export function DesktopPicker({ onConnect }: Props) {
  const [desktops, setDesktops] = useState<DiscoveredDesktop[]>([]);
  const [scanning, setScanning] = useState(false);
  const [selected, setSelected] = useState<string | null>(null);
  const [manual, setManual] = useState(false);

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
    <View style={styles.card}>
      <View style={styles.head}>
        <Text style={styles.title}>Computers on this network</Text>
        <Pressable onPress={refresh} disabled={scanning} hitSlop={12}>
          {scanning ? <ActivityIndicator color={colors.muted} /> : <Text style={styles.link}>Refresh</Text>}
        </Pressable>
      </View>

      {desktops.length === 0 && !scanning && (
        <Text style={styles.muted}>
          None found. Make sure Celmic is open on your computer and both are on the same Wi-Fi.
        </Text>
      )}

      {desktops.map((desktop) => {
        const open = selected === desktop.name;
        return (
          <View key={desktop.name} style={styles.row}>
            <Pressable onPress={() => setSelected(open ? null : desktop.name)} style={styles.rowHead}>
              <View style={styles.dot} />
              <View style={{ flex: 1 }}>
                <Text style={styles.name}>{desktop.name}</Text>
                <Text style={styles.sub}>{desktop.hosts[0]}</Text>
              </View>
              <Text style={styles.chevron}>{open ? "–" : "+"}</Text>
            </Pressable>
            {open && (
              <CodeInput
                onComplete={(code) =>
                  onConnect({ hosts: desktop.hosts, port: desktop.port, code, name: desktop.name })
                }
              />
            )}
          </View>
        );
      })}

      <Pressable onPress={() => setManual((m) => !m)} hitSlop={8}>
        <Text style={styles.link}>{manual ? "Hide manual connection" : "Enter address manually"}</Text>
      </Pressable>
      {manual && <ManualForm onConnect={onConnect} />}
    </View>
  );
}

function CodeInput({ onComplete }: { onComplete: (code: string) => void }) {
  const [code, setCode] = useState("");
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
      placeholder="Pairing code"
      placeholderTextColor={colors.muted}
      maxLength={PAIRING_CODE_LENGTH}
      style={[styles.input, styles.codeInput]}
    />
  );
}

function ManualForm({ onConnect }: Props) {
  const [host, setHost] = useState("");
  const [port, setPort] = useState(String(DEFAULT_PORT));
  const [code, setCode] = useState("");
  const portNumber = Number(port);
  const valid =
    isIPv4(host.trim()) &&
    /^\d+$/.test(port) &&
    portNumber >= 1 &&
    portNumber <= 65535 &&
    isValidPairingCode(code);

  return (
    <View style={styles.manual}>
      <TextInput
        value={host}
        onChangeText={setHost}
        placeholder="192.168.0.10"
        placeholderTextColor={colors.muted}
        keyboardType="decimal-pad"
        autoCorrect={false}
        style={styles.input}
      />
      <View style={styles.manualRow}>
        <TextInput
          value={port}
          onChangeText={(t) => setPort(t.replace(/\D/g, "").slice(0, 5))}
          keyboardType="number-pad"
          style={[styles.input, { flex: 1 }]}
        />
        <TextInput
          value={code}
          onChangeText={(t) => setCode(t.replace(/\D/g, "").slice(0, PAIRING_CODE_LENGTH))}
          placeholder="Code"
          placeholderTextColor={colors.muted}
          keyboardType="number-pad"
          style={[styles.input, { flex: 1.4 }]}
        />
      </View>
      <Pressable
        disabled={!valid}
        onPress={() => onConnect({ hosts: [host.trim()], port: portNumber, code })}
        style={[styles.button, !valid && { opacity: 0.4 }]}
      >
        <Text style={styles.buttonText}>Connect</Text>
      </Pressable>
    </View>
  );
}

const styles = StyleSheet.create({
  card: {
    gap: 12,
    padding: 16,
    borderRadius: 16,
    borderWidth: 1,
    borderColor: colors.border,
    backgroundColor: colors.surface,
  },
  head: { flexDirection: "row", alignItems: "center", justifyContent: "space-between" },
  title: { color: colors.muted, fontSize: 12, fontWeight: "600", letterSpacing: 0.6, textTransform: "uppercase" },
  muted: { color: colors.muted, fontSize: 14, lineHeight: 20 },
  link: { color: colors.accent, fontSize: 14, fontWeight: "600" },
  row: { gap: 10, paddingVertical: 4 },
  rowHead: { flexDirection: "row", alignItems: "center", gap: 12 },
  dot: { width: 8, height: 8, borderRadius: 4, backgroundColor: colors.accent },
  name: { color: colors.text, fontSize: 16, fontWeight: "600" },
  sub: { color: colors.muted, fontSize: 13, fontFamily: mono },
  chevron: { color: colors.muted, fontSize: 22, width: 24, textAlign: "center" },
  input: {
    backgroundColor: colors.surface2,
    color: colors.text,
    borderRadius: 10,
    paddingHorizontal: 14,
    paddingVertical: 12,
    fontSize: 16,
  },
  codeInput: { fontFamily: mono, fontSize: 22, letterSpacing: 8, textAlign: "center" },
  manual: { gap: 10 },
  manualRow: { flexDirection: "row", gap: 10 },
  button: { backgroundColor: colors.accent, borderRadius: 12, paddingVertical: 14, alignItems: "center" },
  buttonText: { color: colors.accentInk, fontWeight: "700", fontSize: 16 },
});
