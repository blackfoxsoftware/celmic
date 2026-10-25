import { DarkTheme, Stack, ThemeProvider } from "expo-router";
import { StatusBar } from "expo-status-bar";

import { colors } from "@/lib/theme";

const theme = {
  ...DarkTheme,
  colors: { ...DarkTheme.colors, background: colors.bg, card: colors.bg, primary: colors.accent },
};

export default function RootLayout() {
  return (
    <ThemeProvider value={theme}>
      <StatusBar style="light" />
      <Stack screenOptions={{ headerShown: false, contentStyle: { backgroundColor: colors.bg } }}>
        <Stack.Screen name="index" />
        <Stack.Screen
          name="scan"
          options={{ presentation: "modal", headerShown: true, title: "Scan pairing code" }}
        />
        <Stack.Screen name="connect" />
      </Stack>
    </ThemeProvider>
  );
}
