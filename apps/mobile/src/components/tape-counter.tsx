import { useEffect, useRef } from "react";
import { Animated, Easing, View } from "react-native";

import { Text } from "@/components/ui/text";
import { cn } from "@/lib/utils";

const DIGITS = "0123456789";

/** One wheel of a cassette deck's counter, rolling to its new digit. */
function Wheel({ digit, size }: { digit: number; size: number }) {
  const cell = Math.round(size * 1.3);
  const y = useRef(new Animated.Value(-digit * cell)).current;

  useEffect(() => {
    Animated.timing(y, {
      toValue: -digit * cell,
      duration: 500,
      easing: Easing.out(Easing.exp),
      useNativeDriver: true,
    }).start();
  }, [digit, cell, y]);

  return (
    <View className="overflow-hidden rounded-sm bg-spine" style={{ height: cell, width: Math.round(size * 0.95) }}>
      <Animated.View style={{ transform: [{ translateY: y }] }}>
        {DIGITS.split("").map((d) => (
          <Text key={d} className="text-center font-mono text-foreground" style={{ fontSize: size, lineHeight: cell, height: cell }}>
            {d}
          </Text>
        ))}
      </Animated.View>
    </View>
  );
}

/** Figures on a tape counter: digits roll, anything else is printed flat. */
export function TapeCounter({
  value,
  size,
  label,
  separatorClassName,
}: {
  value: string;
  size: number;
  label?: string;
  separatorClassName?: string;
}) {
  return (
    <View accessible accessibilityLabel={label ?? value} className="flex-row items-center gap-1">
      {value.split("").map((char, i) =>
        DIGITS.includes(char) ? (
          <Wheel key={i} digit={Number(char)} size={size} />
        ) : (
          <Text key={i} className={cn("font-mono", separatorClassName)} style={{ fontSize: size * 0.8 }}>
            {char}
          </Text>
        ),
      )}
    </View>
  );
}

export function formatCounter(totalSecs: number) {
  const h = Math.floor(totalSecs / 3600);
  const m = Math.floor((totalSecs % 3600) / 60);
  const s = totalSecs % 60;
  const pad = (n: number) => n.toString().padStart(2, "0");
  return h > 0 ? `${h}:${pad(m)}:${pad(s)}` : `${pad(m)}:${pad(s)}`;
}
