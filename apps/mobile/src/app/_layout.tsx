import "../global.css";

import { useEffect } from "react";
import { DarkTheme, Stack, ThemeProvider } from "expo-router";
import * as SplashScreen from "expo-splash-screen";
import { StatusBar } from "expo-status-bar";
import { useFonts } from "expo-font";
import { Barlow_400Regular, Barlow_500Medium, Barlow_600SemiBold } from "@expo-google-fonts/barlow";
import { BarlowCondensed_600SemiBold, BarlowCondensed_700Bold } from "@expo-google-fonts/barlow-condensed";
import { JetBrainsMono_500Medium, JetBrainsMono_700Bold } from "@expo-google-fonts/jetbrains-mono";

import { useColor } from "@/lib/on-air";

void SplashScreen.preventAutoHideAsync();

export default function RootLayout() {
  const [fontsLoaded, fontError] = useFonts({
    Barlow_400Regular,
    Barlow_500Medium,
    Barlow_600SemiBold,
    BarlowCondensed_600SemiBold,
    BarlowCondensed_700Bold,
    JetBrainsMono_500Medium,
    JetBrainsMono_700Bold,
  });
  const background = useColor("background");
  const foreground = useColor("foreground");
  const primary = useColor("primary");

  useEffect(() => {
    if (fontsLoaded || fontError) void SplashScreen.hideAsync();
  }, [fontsLoaded, fontError]);

  if (!fontsLoaded && !fontError) return null;

  const theme = {
    ...DarkTheme,
    colors: { ...DarkTheme.colors, background, card: background, text: foreground, primary },
  };

  return (
    <ThemeProvider value={theme}>
      <StatusBar style="light" />
      <Stack screenOptions={{ headerShown: false, contentStyle: { backgroundColor: background } }}>
        <Stack.Screen name="index" />
        <Stack.Screen
          name="scan"
          options={{
            presentation: "modal",
            headerShown: true,
            title: "Scan the pairing code",
            headerTitleStyle: { fontFamily: "BarlowCondensed_700Bold", fontSize: 22 },
          }}
        />
        <Stack.Screen name="connect" />
      </Stack>
    </ThemeProvider>
  );
}
