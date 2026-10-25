import { describe, expect, test } from "bun:test";
import { buildConnectUrl, isIPv4, parseConnectParams, parseConnectUrl } from "./index";

describe("connect url", () => {
  test("round-trips", () => {
    const target = { hosts: ["192.168.0.10", "10.0.0.4"], port: 47800, code: "012345", name: "My PC" };
    expect(parseConnectUrl(buildConnectUrl(target))).toEqual(target);
  });

  test("rejects foreign urls and bad codes", () => {
    expect(parseConnectUrl("https://example.com/?h=1.2.3.4&p=1&c=123456")).toBeNull();
    expect(parseConnectUrl("celmic://connect?h=1.2.3.4&p=47800&c=12")).toBeNull();
    expect(parseConnectUrl("celmic://connect?h=nope&p=47800&c=123456")).toBeNull();
    expect(parseConnectUrl("celmic://connect")).toBeNull();
  });

  test("parses router-style params", () => {
    expect(parseConnectParams({ h: "1.2.3.4", p: "5000", c: "999999" })).toEqual({
      hosts: ["1.2.3.4"],
      port: 5000,
      code: "999999",
    });
  });

  test("ipv4", () => {
    expect(isIPv4("192.168.1.1")).toBe(true);
    expect(isIPv4("256.1.1.1")).toBe(false);
    expect(isIPv4("fe80::1")).toBe(false);
  });
});
