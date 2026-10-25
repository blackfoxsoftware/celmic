import { useRef, useState } from "react";
import { Pressable, StyleSheet, Text, View } from "react-native";
import { CameraView, useCameraPermissions } from "expo-camera";
import { router } from "expo-router";
import { parseConnectUrl } from "@celmic/protocol";

import { connect } from "@/lib/stream";
import { colors } from "@/lib/theme";

export default function ScanScreen() {
  const [permission, requestPermission] = useCameraPermissions();
  const [hint, setHint] = useState("Point at the QR code in the Celmic desktop app.");
  const handled = useRef(false);

  if (!permission) return <View style={styles.fill} />;

  if (!permission.granted) {
    return (
      <View style={[styles.fill, styles.center]}>
        <Text style={styles.text}>Camera access is needed to scan the pairing code.</Text>
        <Pressable style={styles.button} onPress={requestPermission}>
          <Text style={styles.buttonText}>Allow camera</Text>
        </Pressable>
      </View>
    );
  }

  return (
    <View style={styles.fill}>
      <CameraView
        style={StyleSheet.absoluteFill}
        facing="back"
        barcodeScannerSettings={{ barcodeTypes: ["qr"] }}
        onBarcodeScanned={({ data }) => {
          if (handled.current) return;
          const target = parseConnectUrl(data);
          if (!target) {
            setHint("That isn't a Celmic code.");
            return;
          }
          handled.current = true;
          void connect(target);
          router.back();
        }}
      />
      <View style={styles.frame} pointerEvents="none" />
      <Text style={styles.hint}>{hint}</Text>
    </View>
  );
}

const styles = StyleSheet.create({
  fill: { flex: 1, backgroundColor: "#000" },
  center: { alignItems: "center", justifyContent: "center", gap: 16, padding: 24 },
  text: { color: colors.text, fontSize: 16, textAlign: "center" },
  button: { backgroundColor: colors.accent, borderRadius: 12, paddingHorizontal: 20, paddingVertical: 12 },
  buttonText: { color: colors.accentInk, fontWeight: "700", fontSize: 16 },
  frame: {
    position: "absolute",
    alignSelf: "center",
    top: "25%",
    width: 240,
    height: 240,
    borderRadius: 24,
    borderWidth: 3,
    borderColor: colors.accent,
  },
  hint: {
    position: "absolute",
    bottom: 64,
    alignSelf: "center",
    color: "#fff",
    fontSize: 15,
    backgroundColor: "rgba(0,0,0,0.6)",
    paddingHorizontal: 14,
    paddingVertical: 8,
    borderRadius: 999,
    overflow: "hidden",
  },
});
