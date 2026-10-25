import { Platform } from "react-native";

export const colors = {
  bg: "#0C0E12",
  surface: "#14171D",
  surface2: "#1B1F27",
  border: "#252A34",
  text: "#E8EAF0",
  muted: "#8A91A0",
  accent: "#5EEAD4",
  accentInk: "#062823",
  warn: "#F5C27A",
  danger: "#F87171",
} as const;

export const mono = Platform.select({ ios: "Menlo", default: "monospace" });
