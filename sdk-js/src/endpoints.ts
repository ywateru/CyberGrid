import { CyberGridError } from "./index.js";
export interface EndpointProvider {
  readonly id: string;
  readonly origins: readonly string[];
  readonly paths: readonly string[];
}
export interface EndpointPolicy {
  readonly version: 1;
  readonly policyVersion: string;
  readonly providers: readonly EndpointProvider[];
}
export interface BridgeLimits {
  readonly maxFrameBytes: number;
  readonly maxStreams: number;
  readonly maxPendingRequests: number;
}
export interface BrowserBridge {
  readonly id: string;
  readonly protocol: string;
  readonly providerId: string;
  readonly url: string;
  readonly installationId: string;
  readonly publicKey: string;
  readonly addressRevision: number;
  readonly expiresAt: string;
  readonly limits: BridgeLimits;
}
export function invalid(): never {
  throw new CyberGridError("INVALID_RESPONSE", undefined, {
    stage: "directory",
    retryable: false,
  });
}
export function obj(v: unknown): Record<string, unknown> {
  if (!v || typeof v !== "object" || Array.isArray(v)) return invalid();
  return v as Record<string, unknown>;
}
function exact(v: Record<string, unknown>, keys: string[]) {
  if (
    Object.keys(v).length !== keys.length ||
    keys.some((k) => !Object.hasOwn(v, k))
  )
    invalid();
}
export function identifier(v: unknown, max: number, protocol = false): string {
  if (
    typeof v !== "string" ||
    !v.length ||
    v.length > max ||
    !(protocol ? /^[A-Za-z0-9_.-]+$/ : /^[A-Za-z0-9_-]+$/).test(v)
  )
    return invalid();
  return v;
}
function validPath(v: string) {
  return (
    v.length <= 256 &&
    /^\/[A-Za-z0-9_~.!$&'()*+,;=:@/-]*$/.test(v) &&
    !v.includes("//") &&
    !v.split("/").some((s) => s === "." || s === "..")
  );
}
export function deepFreeze<T>(v: T): T {
  if (v && typeof v === "object") {
    for (const child of Object.values(v)) deepFreeze(child);
    Object.freeze(v);
  }
  return v;
}
export async function policyConfiguration(
  value: unknown,
): Promise<EndpointPolicy> {
  let providers: EndpointProvider[] = [];
  let version: string | undefined;
  if (value !== undefined) {
    const p = obj(value);
    exact(p, ["version", "policyVersion", "providers"]);
    if (
      p.version !== 1 ||
      typeof p.policyVersion !== "string" ||
      !/^[a-f0-9]{64}$/.test(p.policyVersion) ||
      !Array.isArray(p.providers) ||
      p.providers.length > 16 ||
      new TextEncoder().encode(JSON.stringify(p.providers)).length > 32768
    )
      return invalid();
    version = p.policyVersion;
    providers = p.providers
      .map((item) => {
        const e = obj(item);
        exact(e, ["id", "origins", "paths"]);
        const id = identifier(e.id, 64);
        if (
          id === "quick-tunnel" ||
          !Array.isArray(e.origins) ||
          !e.origins.length ||
          e.origins.length > 16 ||
          !Array.isArray(e.paths) ||
          !e.paths.length ||
          e.paths.length > 16
        )
          return invalid();
        const origins = e.origins.map((origin) => {
          if (typeof origin !== "string" || /[^\x21-\x7e]/.test(origin))
            return invalid();
          let u: URL;
          try {
            u = new URL(origin);
          } catch {
            return invalid();
          }
          if (
            u.protocol !== "https:" ||
            u.origin !== origin ||
            u.username ||
            u.password ||
            u.pathname !== "/" ||
            u.search ||
            u.hash
          )
            return invalid();
          return origin;
        });
        const paths = e.paths.map((path) => {
          if (typeof path !== "string" || !validPath(path)) return invalid();
          return path;
        });
        if (
          new Set(origins).size !== origins.length ||
          new Set(paths).size !== paths.length
        )
          return invalid();
        return { id, origins: origins.sort(), paths: paths.sort() };
      })
      .sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));
    if (new Set(providers.map((p) => p.id)).size !== providers.length)
      return invalid();
  }
  const bytes = new TextEncoder().encode(
    JSON.stringify([
      "cybergrid-endpoint-policy-v1",
      providers.map((p) => [p.id, p.origins, p.paths]),
    ]),
  );
  const digest = Array.from(
    new Uint8Array(await crypto.subtle.digest("SHA-256", bytes)),
    (v) => v.toString(16).padStart(2, "0"),
  ).join("");
  if (version !== undefined && version !== digest) return invalid();
  return deepFreeze({ version: 1, policyVersion: digest, providers });
}
export function endpoint(
  value: unknown,
  policy: EndpointPolicy,
  bridgeProvider?: string,
  device = false,
): string | null {
  if (value === null || value === undefined) return null;
  if (
    typeof value !== "string" ||
    !value.length ||
    value.length > 1024 ||
    /[^\x21-\x7e]/.test(value) ||
    /[\\%?#]/.test(value)
  )
    return invalid();
  let u: URL;
  try {
    u = new URL(value);
  } catch {
    return invalid();
  }
  const scheme = bridgeProvider === undefined ? "https:" : "wss:";
  if (
    u.protocol !== scheme ||
    u.username ||
    u.password ||
    u.search ||
    u.hash ||
    !validPath(u.pathname) ||
    (value !== u.href &&
      !(scheme === "https:" && u.pathname === "/" && value === u.origin)) ||
    (device && u.pathname !== "/")
  )
    return invalid();
  const quick =
    u.hostname.split(".")[0]!.length <= 63 &&
    /^([a-z0-9]+-)*[a-z0-9]+\.trycloudflare\.com$/.test(u.hostname) &&
    !u.port;
  if (bridgeProvider === "quick-tunnel") {
    if (!quick) return invalid();
    return u.href;
  }
  if (bridgeProvider === undefined && quick && u.pathname === "/")
    return u.origin;
  const origin = "https://" + u.host;
  if (
    !policy.providers.some(
      (p) =>
        (bridgeProvider === undefined || p.id === bridgeProvider) &&
        p.origins.includes(origin) &&
        p.paths.includes(u.pathname),
    )
  )
    return invalid();
  return u.pathname === "/" && scheme === "https:" ? u.origin : u.href;
}
export function parseBridge(
  value: unknown,
  policy: EndpointPolicy,
): BrowserBridge {
  const b = obj(value);
  exact(b, [
    "id",
    "protocol",
    "providerId",
    "url",
    "installationId",
    "publicKey",
    "addressRevision",
    "expiresAt",
    "limits",
  ]);
  const id = identifier(b.id, 64),
    protocol = identifier(b.protocol, 128, true),
    providerId = identifier(b.providerId, 64),
    installationId = identifier(b.installationId, 128);
  if (
    typeof b.publicKey !== "string" ||
    !/^[A-Za-z0-9_-]{42}[AEIMQUYcgkosw048]$/.test(b.publicKey)
  )
    return invalid();
  if (
    !Number.isSafeInteger(b.addressRevision) ||
    (b.addressRevision as number) < 1 ||
    typeof b.expiresAt !== "string" ||
    !Number.isFinite(Date.parse(b.expiresAt)) ||
    new Date(b.expiresAt).toISOString() !== b.expiresAt
  )
    return invalid();
  const l = obj(b.limits);
  exact(l, ["maxFrameBytes", "maxStreams", "maxPendingRequests"]);
  for (const [k, max] of [
    ["maxFrameBytes", 65535],
    ["maxStreams", 32],
    ["maxPendingRequests", 128],
  ] as const)
    if (
      !Number.isSafeInteger(l[k]) ||
      (l[k] as number) < 1 ||
      (l[k] as number) > max
    )
      return invalid();
  const url = endpoint(b.url, policy, providerId);
  if (!url) return invalid();
  return deepFreeze({
    id,
    protocol,
    providerId,
    url,
    installationId,
    publicKey: b.publicKey,
    addressRevision: b.addressRevision as number,
    expiresAt: b.expiresAt,
    limits: {
      maxFrameBytes: l.maxFrameBytes as number,
      maxStreams: l.maxStreams as number,
      maxPendingRequests: l.maxPendingRequests as number,
    },
  });
}
