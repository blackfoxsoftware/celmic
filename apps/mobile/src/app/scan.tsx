import { useRef, useState } from "react";
import { StyleSheet, View } from "react-native";
import { CameraView, useCameraPermissions } from "expo-camera";
import { router } from "expo-router";
import { parseConnectUrl } from "@celmic/protocol";

import { Button } from "@/components/ui/button";
import { Text } from "@/components/ui/text";
import { connect } from "@/lib/stream";

/** Corner marks of the viewfinder, in the index ink you can press. */
function Corner({ position }: { position: "tl" | "tr" | "bl" | "br" }) {
  const v = position[0] === "t" ? "top-0 border-t-4" : "bottom-0 border-b-4";
  const h = position[1] === "l" ? "left-0 border-l-4" : "right-0 border-r-4";
  return <View className={`absolute size-10 border-live ${v} ${h}`} />;
}

export default function ScanScreen() {
  const [permission, requestPermission] = useCameraPermissions();
  const [hint, setHint] = useState("Point at the QR code in celmic on your PC.");
  const handled = useRef(false);

  if (!permission) return <View className="flex-1 bg-background" />;

  if (!permission.granted) {
    return (
      <View className="flex-1 items-center justify-center gap-5 bg-background p-6">
        <Text className="text-center text-lg">celmic needs the camera to read the pairing code.</Text>
        <Button onPress={requestPermission}>
          <Text>Allow camera</Text>
        </Button>
      </View>
    );
  }

  return (
    <View className="flex-1 bg-black">
      <CameraView
        style={StyleSheet.absoluteFill}
        facing="back"
        barcodeScannerSettings={{ barcodeTypes: ["qr"] }}
        onBarcodeScanned={({ data }) => {
          if (handled.current) return;
          const target = parseConnectUrl(data);
          if (!target) {
            setHint("That isn't a celmic code.");
            return;
          }
          handled.current = true;
          void connect(target);
          router.back();
        }}
      />
      <View pointerEvents="none" className="absolute top-[25%] size-64 self-center">
        <Corner position="tl" />
        <Corner position="tr" />
        <Corner position="bl" />
        <Corner position="br" />
      </View>
      <View className="absolute bottom-16 self-center rounded-md bg-background/85 px-4 py-2">
        <Text className="font-sans-medium">{hint}</Text>
      </View>
    </View>
  );
}
