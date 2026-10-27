import type { ReactNode } from "react";
import { View } from "react-native";

import { Text } from "@/components/ui/text";
import { INK_CLASS, type OnAir } from "@/lib/on-air";
import { cn } from "@/lib/utils";

/**
 * The J-card index stripe, shared with the desktop: its ink is the state, the
 * tag reads like the cassette's side label, the computer's name is written
 * large, and the tape counter sits on the right.
 */
export function IndexStripe({ state, counter, topInset }: { state: OnAir; counter?: ReactNode; topInset: number }) {
  const ink = INK_CLASS[state.ink];
  return (
    <View
      accessibilityLiveRegion="polite"
      className={cn("gap-3 px-5 pb-6", ink.bg)}
      style={{ paddingTop: topInset + 14 }}
    >
      <View className="flex-row items-center justify-between">
        <View className="flex-row items-center gap-2">
          <View className={cn("size-3 rounded-full", state.ink === "wait" ? "bg-wait-foreground" : "bg-background")} />
          <Text className={cn("font-display text-lg uppercase tracking-[0.2em]", ink.text)}>{state.tag}</Text>
        </View>
        {counter}
      </View>
      <Text numberOfLines={2} className={cn("font-display text-[44px] leading-[42px] tracking-tight", ink.text)}>
        {state.title}
      </Text>
      <Text numberOfLines={2} className={cn("font-sans-medium text-base", ink.muted)}>
        {state.detail}
      </Text>
    </View>
  );
}
