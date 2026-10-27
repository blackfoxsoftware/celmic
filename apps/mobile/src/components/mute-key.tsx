import { Pressable, View } from "react-native";
import * as Haptics from "expo-haptics";
import { Mic, MicOff } from "lucide-react-native";

import { Text } from "@/components/ui/text";
import { useColor } from "@/lib/on-air";
import { cn } from "@/lib/utils";

const SEGMENTS = 24;
const FLOOR_DB = -60;
const HOT_DB = -9;

/**
 * The mute key: the biggest thing on the phone while streaming. The voice
 * level runs along its foot as a deck's segment meter.
 */
export function MuteKey({ muted, level, onToggle }: { muted: boolean; level: number; onToggle: () => void }) {
  const ink = useColor(muted ? "ready-foreground" : "foreground");
  const db = level > 0 ? 20 * Math.log10(level) : -Infinity;
  const lit = muted ? 0 : Math.round(Math.max(0, Math.min(1, (db - FLOOR_DB) / -FLOOR_DB)) * SEGMENTS);
  const Icon = muted ? MicOff : Mic;

  return (
    <Pressable
      accessibilityRole="switch"
      accessibilityState={{ checked: muted }}
      accessibilityLabel={muted ? "Microphone muted. Tap to talk." : "Microphone on. Tap to mute."}
      onPress={() => {
        void Haptics.impactAsync(Haptics.ImpactFeedbackStyle.Medium);
        onToggle();
      }}
      className={cn(
        "flex-1 justify-between rounded-lg border px-6 pb-6 pt-8",
        muted ? "border-ready bg-ready active:bg-ready/90" : "border-input bg-secondary active:bg-accent",
      )}
    >
      <View className="flex-1 items-center justify-center gap-5">
        <Icon size={88} color={ink} strokeWidth={1.5} />
        <Text className={cn("font-display text-[34px] uppercase tracking-[0.12em]", muted && "text-ready-foreground")}>
          {muted ? "Muted" : "Tap to mute"}
        </Text>
        <Text className={cn("font-sans-medium text-base", muted ? "text-ready-foreground/85" : "text-muted-foreground")}>
          {muted ? "Tap again to talk" : "Your voice is going to the PC"}
        </Text>
      </View>
      <View className="h-6 flex-row gap-1" accessibilityElementsHidden importantForAccessibility="no-hide-descendants">
        {Array.from({ length: SEGMENTS }, (_, i) => {
          const segmentDb = FLOOR_DB + ((i + 1) / SEGMENTS) * -FLOOR_DB;
          const on = i < lit;
          return (
            <View
              key={i}
              className={cn(
                "flex-1 rounded-[2px]",
                muted ? "bg-ready-foreground/15" : !on ? "bg-background" : segmentDb > HOT_DB ? "bg-ready" : "bg-foreground",
              )}
            />
          );
        })}
      </View>
    </Pressable>
  );
}
