import { type Device, type Port } from "./index.js";
import {
  type BrowserBridge,
  type EndpointPolicy,
  deepFreeze,
  endpoint,
  identifier,
  invalid,
  obj,
  parseBridge,
} from "./endpoints.js";
export interface DirectoryDevice extends Omit<Device, "ports"> {
  readonly ports: readonly Readonly<Port>[];
  readonly tunnelUrl: string | null;
  readonly certificateFingerprint: string | null;
  readonly incarnation: string;
  readonly publishRevision: number;
  readonly bridges: readonly BrowserBridge[];
}
export interface DirectorySnapshot {
  readonly version: 1;
  readonly productId: string;
  readonly epoch: string;
  readonly endpointPolicyVersion: string;
  readonly revision: string;
  readonly sampledAt: string;
  readonly reset: boolean;
  readonly changed: boolean;
  readonly devices: readonly DirectoryDevice[];
}
export type DirectoryEvent =
  | {
      readonly type: "snapshot" | "reset";
      readonly eventRevision: number;
      readonly generation: number;
      readonly snapshot: DirectorySnapshot;
    }
  | {
      readonly type: "unknown";
      readonly eventRevision: number;
      readonly generation: number;
      readonly error: import("./index.js").CyberGridError;
    };
export interface DirectoryWatchOptions {
  signal?: AbortSignal;
  intervalMs?: number;
}
export interface DirectoryWatch {
  stop(): void;
  readonly done: Promise<void>;
}
export const isUUID = (v: unknown): v is string =>
  typeof v === "string" &&
  /^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/.test(v);
function string(v: unknown, max: number): string {
  if (
    typeof v !== "string" ||
    !v.length ||
    v.length > max ||
    /[\x00-\x1f\x7f]/.test(v)
  )
    return invalid();
  return v;
}
function unique<T>(values: readonly T[], key: (v: T) => string) {
  if (new Set(values.map(key)).size !== values.length) invalid();
}
export function parseDevices(
  value: unknown,
  policy: EndpointPolicy,
  complete = false,
): DirectoryDevice[] {
  if (!Array.isArray(value) || value.length > 5000) return invalid();
  const devices = value.map((item) => {
    const d = obj(item);
    if (
      typeof d.online !== "boolean" ||
      !Array.isArray(d.ports) ||
      d.ports.length > 32 ||
      (complete && Object.hasOwn(d, "iroh"))
    )
      return invalid();
    const id = string(d.id, 96),
      name = string(d.name, 96),
      lastSeen = string(d.lastSeen, 40);
    if (!Number.isFinite(Date.parse(lastSeen))) return invalid();
    const ports: Port[] = d.ports.map((item) => {
      const p = obj(item);
      if (
        p.transport !== "tcp" ||
        !Number.isInteger(p.port) ||
        (p.port as number) < 1 ||
        (p.port as number) > 65535
      )
        return invalid();
      return {
        id: string(p.id, 64),
        port: p.port as number,
        type: string(p.type, 128),
        transport: "tcp",
        tunnelUrl: endpoint(p.tunnelUrl, policy),
      };
    });
    unique(ports, (p) => p.id);
    if (
      complete &&
      (!isUUID(d.incarnation) ||
        !Number.isSafeInteger(d.publishRevision) ||
        (d.publishRevision as number) < 0 ||
        !Array.isArray(d.bridges))
    )
      return invalid();
    const incarnation =
      d.incarnation === undefined
        ? ""
        : isUUID(d.incarnation)
          ? d.incarnation
          : invalid();
    const publishRevision =
      d.publishRevision === undefined
        ? 0
        : Number.isSafeInteger(d.publishRevision) &&
            (d.publishRevision as number) >= 0
          ? (d.publishRevision as number)
          : invalid();
    const fingerprint =
      d.certificateFingerprint === undefined ||
      d.certificateFingerprint === null
        ? null
        : typeof d.certificateFingerprint === "string" &&
            /^[a-f0-9]{64}$/.test(d.certificateFingerprint)
          ? d.certificateFingerprint
          : invalid();
    const raw = d.bridges === undefined ? [] : d.bridges;
    if (!Array.isArray(raw) || raw.length > 4) return invalid();
    const bridges = raw.map((v) => parseBridge(v, policy));
    unique(bridges, (b) => b.id);
    bridges.sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));
    return {
      id,
      name,
      online: d.online,
      lastSeen,
      ports,
      tunnelUrl: endpoint(d.tunnelUrl, policy, undefined, true),
      certificateFingerprint: fingerprint,
      incarnation,
      publishRevision,
      bridges,
    };
  });
  unique(devices, (d) => d.id);
  return devices.sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));
}
export function parseDirectory(
  value: unknown,
  productId: string,
  epoch: string,
  policy: EndpointPolicy,
): DirectorySnapshot {
  const s = obj(value);
  identifier(productId, 21);
  if (
    s.version !== 1 ||
    s.productId !== productId ||
    s.epoch !== epoch ||
    s.endpointPolicyVersion !== policy.policyVersion ||
    typeof s.reset !== "boolean" ||
    typeof s.changed !== "boolean" ||
    typeof s.revision !== "string" ||
    s.revision.length > 512 ||
    !/^v1\.[a-f0-9-]{36}\.[a-f0-9]{64}\.[a-f0-9]{64}$/.test(s.revision) ||
    !s.revision.startsWith("v1." + epoch + ".") ||
    typeof s.sampledAt !== "string" ||
    !Number.isFinite(Date.parse(s.sampledAt)) ||
    new Date(s.sampledAt).toISOString() !== s.sampledAt
  )
    return invalid();
  const devices = parseDevices(s.devices, policy, true);
  for (const d of devices)
    for (const b of d.bridges) {
      const delta = Date.parse(b.expiresAt) - Date.parse(s.sampledAt);
      if (delta <= 0 || delta > 300000) return invalid();
    }
  return deepFreeze({
    version: 1,
    productId,
    epoch,
    endpointPolicyVersion: policy.policyVersion,
    revision: s.revision,
    sampledAt: s.sampledAt,
    reset: s.reset,
    changed: s.changed,
    devices,
  });
}
