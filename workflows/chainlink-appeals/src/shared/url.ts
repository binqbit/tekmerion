import { check } from "@tekmerion/sdk/wire";
import PortableURL from "core-js-pure/actual/url/index.js";

/** Javy has no native URL; keep WHATWG parsing independent of host globals. */
export function parseUrl(value: string): URL {
  return new PortableURL(value);
}

export function publicUrl(value: unknown): string {
  check(typeof value === "string" && value.length <= 2048, "Invalid URL");
  const url = parseUrl(value);
  check(
    url.protocol === "https:" &&
      !url.username &&
      !url.password &&
      !url.hash &&
      !url.port &&
      url.href === value &&
      /^[a-z0-9.-]+\.[a-z]{2,}$/.test(url.hostname) &&
      !url.hostname.endsWith(".localhost") &&
      !url.hostname.endsWith(".local"),
    "Only reviewed public HTTPS URLs are allowed",
  );
  return value;
}
