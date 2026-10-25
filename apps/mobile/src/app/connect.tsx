import { useEffect } from "react";
import { Redirect, useLocalSearchParams } from "expo-router";
import { parseConnectParams } from "@celmic/protocol";

import { connect } from "@/lib/stream";

/** Deep link target for `celmic://connect?...` (e.g. QR scanned with the system camera). */
export default function ConnectLink() {
  const params = useLocalSearchParams();

  useEffect(() => {
    const target = parseConnectParams(params);
    if (target) void connect(target);
    // Params are stable for the lifetime of this screen.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return <Redirect href="/" />;
}
