import { Pressable, StyleSheet, Text, View } from "react-native";

import { colors } from "@/lib/theme";

type Props = {
  live: boolean;
  muted: boolean;
  /** Peak level 0..1. */
  level: number;
  onPress?: () => void;
};

const SIZE = 168;

/** Big round mic indicator; the halo follows the input level. Tap to mute while live. */
export function MicOrb({ live, muted, level, onPress }: Props) {
  const db = level > 0 ? 20 * Math.log10(level) : -60;
  const intensity = live && !muted ? Math.max(0, Math.min(1, (db + 50) / 50)) : 0;
  const tint = !live ? colors.surface2 : muted ? colors.danger : colors.accent;

  return (
    <Pressable
      onPress={onPress}
      disabled={!live}
      accessibilityRole="button"
      accessibilityLabel={muted ? "Unmute microphone" : "Mute microphone"}
      style={styles.wrap}
    >
      <View
        style={[
          styles.halo,
          { backgroundColor: tint, opacity: 0.12 + intensity * 0.3, transform: [{ scale: 1 + intensity * 0.28 }] },
        ]}
      />
      <View style={[styles.orb, { backgroundColor: live ? tint : colors.surface }]}>
        <MicGlyph color={live ? colors.accentInk : colors.muted} crossed={muted} />
      </View>
      {live && <Text style={styles.caption}>{muted ? "Muted · tap to unmute" : "Tap to mute"}</Text>}
    </Pressable>
  );
}

function MicGlyph({ color, crossed }: { color: string; crossed: boolean }) {
  return (
    <View style={styles.glyph}>
      <View style={[styles.capsule, { backgroundColor: color }]} />
      <View style={[styles.cup, { borderColor: color }]} />
      <View style={[styles.stem, { backgroundColor: color }]} />
      {crossed && <View style={[styles.slash, { backgroundColor: color }]} />}
    </View>
  );
}

const styles = StyleSheet.create({
  wrap: { alignItems: "center", justifyContent: "center", height: SIZE * 1.5 },
  halo: { position: "absolute", top: SIZE * 0.15, width: SIZE * 1.2, height: SIZE * 1.2, borderRadius: SIZE },
  orb: {
    width: SIZE,
    height: SIZE,
    borderRadius: SIZE / 2,
    alignItems: "center",
    justifyContent: "center",
    borderWidth: 1,
    borderColor: colors.border,
  },
  caption: { position: "absolute", bottom: 0, color: colors.muted, fontSize: 13 },
  glyph: { width: 56, height: 72, alignItems: "center" },
  capsule: { width: 24, height: 40, borderRadius: 12 },
  cup: {
    position: "absolute",
    top: 18,
    width: 40,
    height: 34,
    borderWidth: 4,
    borderTopWidth: 0,
    borderBottomLeftRadius: 20,
    borderBottomRightRadius: 20,
  },
  stem: { position: "absolute", bottom: 4, width: 4, height: 16, borderRadius: 2 },
  slash: { position: "absolute", top: 34, width: 70, height: 5, borderRadius: 3, transform: [{ rotate: "-45deg" }] },
});
