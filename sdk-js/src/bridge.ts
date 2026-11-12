import { CyberGridError, type RequestOptions } from "./index.js";
import {
  deepFreeze,
  type BrowserBridge,
  type BridgeLimits,
} from "./endpoints.js";
import type {
  DirectorySnapshot,
  DirectoryDevice,
  DirectoryEvent,
  DirectoryWatch,
  DirectoryWatchOptions,
} from "./directory.js";
export interface BridgeTarget {
  readonly userId: string;
  readonly productId: string;
  readonly deviceId: string;
  readonly epoch: string;
  readonly generation: number;
  readonly incarnation: string;
  readonly publishRevision: number;
  readonly endpointPolicyVersion: string;
  readonly descriptor: BrowserBridge;
}
export interface AdapterByteStream {
  readonly readable: ReadableStream<Uint8Array>;
  readonly writable: WritableStream<Uint8Array>;
}
export interface AuthenticatedBridgePeer<Req, Res, StreamInput = unknown> {
  readonly encrypted: true;
  readonly identity: {
    readonly protocol: string;
    readonly publicKey: string;
    readonly installationId: string;
  };
  readonly limits: BridgeLimits;
  request(input: Req, options: { signal: AbortSignal }): Promise<Res>;
  openStream(
    input: StreamInput,
    options: { signal: AbortSignal },
  ): Promise<AdapterByteStream>;
  readonly closed: Promise<void>;
  close(): Promise<void>;
}
export interface TrustedBridgeAdapter<Req, Res, StreamInput = unknown> {
  readonly protocol: string;
  connect(
    target: BridgeTarget,
    context: { readonly signal: AbortSignal; readonly remainingMs: number },
  ): Promise<AuthenticatedBridgePeer<Req, Res, StreamInput>>;
}
export interface BridgeConnectOptions extends RequestOptions {
  timeoutMs?: number;
}
export interface BridgeState {
  readonly phase: "connecting" | "connected" | "closed";
  readonly reason:
    | "cancelled"
    | "expired"
    | "directory-unavailable"
    | "target-changed"
    | "transport-closed"
    | "protocol-error"
    | "closed"
    | null;
  readonly cleanupPending: boolean;
}
export interface SecureBridgeStream {
  read(options?: RequestOptions): Promise<Uint8Array | null>;
  write(bytes: Uint8Array, options?: RequestOptions): Promise<void>;
  close(): Promise<void>;
}
export interface SecureBridge<Req, Res, StreamInput = unknown> {
  request(input: Req, options?: RequestOptions): Promise<Res>;
  openStream(
    input: StreamInput,
    options?: RequestOptions,
  ): Promise<SecureBridgeStream>;
  state(): BridgeState;
  close(): Promise<BridgeState>;
}
interface Host {
  directory(
    product: string,
    options: RequestOptions,
  ): Promise<DirectorySnapshot>;
  watch(
    product: string,
    listener: (event: DirectoryEvent) => void,
    options: DirectoryWatchOptions,
  ): DirectoryWatch;
  sample(s: DirectorySnapshot): { start: number; sequence: number };
  identity(): { generation: number; userId?: string; epoch?: string };
  onInvalidated(close: () => void): () => void;
  release(): void;
}
const error = (
  code:
    | "BRIDGE_CLOSED"
    | "BRIDGE_UNAVAILABLE"
    | "BRIDGE_PROTOCOL_ERROR"
    | "BRIDGE_IDENTITY_MISMATCH"
    | "BRIDGE_CAPACITY_EXCEEDED"
    | "UNSUPPORTED_BRIDGE"
    | "CANCELLED"
    | "TIMEOUT",
) => new CyberGridError(code, undefined, { stage: "bridge", retryable: false });
function raced<T>(
  promise: Promise<T>,
  signals: readonly (AbortSignal | undefined)[],
): Promise<T> {
  return new Promise((resolve, reject) => {
    const remove: (() => void)[] = [];
    let done = false;
    const finish = (value?: T, e?: unknown) => {
      if (done) return;
      done = true;
      for (const off of remove) off();
      if (e) reject(e);
      else resolve(value as T);
    };
    for (const signal of signals) {
      if (done) break;
      if (!signal) continue;
      const abort = () =>
        finish(
          undefined,
          signal.reason instanceof CyberGridError
            ? new CyberGridError(signal.reason.code, signal.reason.status, {
                stage: "bridge",
                retryable: false,
              })
            : error("CANCELLED"),
        );
      signal.addEventListener("abort", abort, { once: true });
      remove.push(() => signal.removeEventListener("abort", abort));
      if (signal.aborted) abort();
    }
    promise.then(
      (v) => finish(v),
      (e) =>
        finish(
          undefined,
          e instanceof CyberGridError
            ? new CyberGridError(e.code, e.status, {
                stage: "bridge",
                retryable: false,
              })
            : error("BRIDGE_CLOSED"),
        ),
    );
  });
}
const binding = (b: BrowserBridge) =>
  JSON.stringify([
    b.id,
    b.protocol,
    b.providerId,
    b.url,
    b.installationId,
    b.publicKey,
    b.addressRevision,
    b.limits,
  ]);
function selected(
  s: DirectorySnapshot,
  device: string,
  id: string,
): { device: DirectoryDevice; bridge: BrowserBridge } {
  const d = s.devices.find((d) => d.id === device),
    b = d?.bridges.find((b) => b.id === id);
  if (!d?.online || !b) throw error("BRIDGE_UNAVAILABLE");
  return { device: d, bridge: b };
}
class Managed<Req, Res, S> implements SecureBridge<Req, Res, S> {
  #state: BridgeState = Object.freeze({
    phase: "connecting",
    reason: null,
    cleanupPending: false,
  });
  #life = new AbortController();
  #peer?: AuthenticatedBridgePeer<Req, Res, S>;
  #watch?: DirectoryWatch;
  #off: () => void;
  #timer?: ReturnType<typeof setTimeout>;
  #deadline = 0;
  #expires = 0;
  #disposed = new WeakMap<object, Promise<void>>();
  #sequence = 0;
  #revision = 0;
  #pending = new Set<Promise<unknown>>();
  #late = new Set<Promise<void>>();
  #streams = new Set<() => Promise<void>>();
  #requests = 0;
  #streamCount = 0;
  #limits: BridgeLimits;
  #cleanup?: Promise<BridgeState>;
  #cleanupFailed = false;
  #target: BridgeTarget;
  constructor(
    readonly host: Host,
    target: BridgeTarget,
  ) {
    this.#target = target;
    this.#limits = target.descriptor.limits;
    this.#off = host.onInvalidated(() => {
      void this.closeWith("directory-unavailable");
    });
  }
  get signal() {
    return this.#life.signal;
  }
  state(): BridgeState {
    return this.#state;
  }
  check() {
    if (this.#life.signal.aborted || this.#state.phase === "closed")
      throw error("BRIDGE_CLOSED");
    if (performance.now() >= this.#deadline) {
      void this.closeWith("expired");
      throw error("BRIDGE_UNAVAILABLE");
    }
  }
  confirm(s: DirectorySnapshot) {
    if (this.#life.signal.aborted) return;
    const sample = this.host.sample(s);
    if (this.#deadline && sample.sequence <= this.#sequence) return;
    const identity = this.host.identity();
    if (
      identity.generation !== this.#target.generation ||
      identity.userId !== this.#target.userId ||
      identity.epoch !== this.#target.epoch ||
      s.epoch !== this.#target.epoch ||
      s.endpointPolicyVersion !== this.#target.endpointPolicyVersion
    ) {
      void this.closeWith("target-changed");
      return;
    }
    let found: ReturnType<typeof selected>;
    try {
      found = selected(s, this.#target.deviceId, this.#target.descriptor.id);
    } catch {
      void this.closeWith("target-changed");
      return;
    }
    if (
      found.device.incarnation !== this.#target.incarnation ||
      binding(found.bridge) !== binding(this.#target.descriptor)
    ) {
      void this.closeWith("target-changed");
      return;
    }
    const deadline =
      sample.start +
      Math.max(0, Date.parse(found.bridge.expiresAt) - Date.parse(s.sampledAt));
    const expires = Date.parse(found.bridge.expiresAt);
    if (
      this.#deadline === 0 ||
      (sample.sequence > this.#sequence &&
        found.device.publishRevision > this.#revision)
    ) {
      if (this.#deadline && expires < this.#expires) {
        void this.closeWith("target-changed");
        return;
      }
      this.#deadline =
        this.#deadline === 0 || expires > this.#expires
          ? deadline
          : Math.min(this.#deadline, deadline);
      this.#expires = expires;
      this.#sequence = sample.sequence;
      this.#revision = found.device.publishRevision;
      if (this.#timer) clearTimeout(this.#timer);
      this.#timer = setTimeout(
        () => {
          void this.closeWith("expired");
        },
        Math.max(0, this.#deadline - performance.now()),
      );
    }
    this.#sequence = sample.sequence;
    if (performance.now() >= this.#deadline) void this.closeWith("expired");
  }
  monitor() {
    this.#watch = this.host.watch(
      this.#target.productId,
      (event) => {
        if (event.type === "unknown")
          void this.closeWith("directory-unavailable");
        else this.confirm(event.snapshot);
      },
      { signal: this.signal },
    );
  }
  ownLate(promise: Promise<void>) {
    this.#late.add(promise);
    promise.then(
      () => this.#late.delete(promise),
      () => {
        this.#cleanupFailed = true; // Unconfirmed cleanup remains owned.
      },
    );
  }
  own<T>(promise: Promise<T>): Promise<T> {
    const settled = promise.then(
      () => {},
      () => {},
    );
    this.#pending.add(settled);
    settled.then(() => this.#pending.delete(settled));
    return promise;
  }
  async install(peer: AuthenticatedBridgePeer<Req, Res, S>) {
    if (this.signal.aborted) {
      await this.disposeLate(peer);
      throw error("CANCELLED");
    }
    this.#peer = peer;
    if (!peer || typeof peer !== "object") throw error("BRIDGE_PROTOCOL_ERROR");
    if (
      peer.encrypted !== true ||
      peer.identity?.protocol !== this.#target.descriptor.protocol ||
      peer.identity.publicKey !== this.#target.descriptor.publicKey ||
      peer.identity.installationId !== this.#target.descriptor.installationId
    )
      throw error("BRIDGE_IDENTITY_MISMATCH");
    const l = peer.limits;
    for (const k of [
      "maxFrameBytes",
      "maxStreams",
      "maxPendingRequests",
    ] as const)
      if (
        !Number.isSafeInteger(l?.[k]) ||
        l[k] < 1 ||
        l[k] > this.#target.descriptor.limits[k]
      )
        throw error("BRIDGE_PROTOCOL_ERROR");
    if (
      typeof peer.request !== "function" ||
      typeof peer.openStream !== "function" ||
      typeof peer.close !== "function" ||
      typeof peer.closed?.then !== "function"
    )
      throw error("BRIDGE_PROTOCOL_ERROR");
    this.#limits = Object.freeze({ ...l });
    this.check();
    this.#state = Object.freeze({
      phase: "connected",
      reason: null,
      cleanupPending: false,
    });
    peer.closed.then(
      () => {
        void this.closeWith("transport-closed");
      },
      () => {
        void this.closeWith("transport-closed");
      },
    );
  }
  disposeLate(peer: AuthenticatedBridgePeer<Req, Res, S>): Promise<void> {
    if (!peer || typeof peer !== "object")
      return Promise.reject(error("BRIDGE_PROTOCOL_ERROR"));
    const old = this.#disposed.get(peer);
    if (old) return old;
    const work = Promise.resolve().then(() => peer.close());
    this.#disposed.set(peer, work);
    return work;
  }
  async request(input: Req, options: RequestOptions = {}): Promise<Res> {
    this.check();
    if (!this.#peer || this.#state.phase !== "connected")
      throw error("BRIDGE_CLOSED");
    if (options.signal?.aborted) throw error("CANCELLED");
    if (this.#requests >= this.#limits.maxPendingRequests)
      throw error("BRIDGE_CAPACITY_EXCEEDED");
    this.#requests++;
    const controller = new AbortController(),
      abort = () => controller.abort(error("CANCELLED"));
    options.signal?.addEventListener("abort", abort, { once: true });
    this.signal.addEventListener("abort", abort, { once: true });
    const raw = this.own(
      Promise.resolve().then(() => {
        this.check();
        if (controller.signal.aborted) throw error("CANCELLED");
        return this.#peer!.request(input, { signal: controller.signal });
      }),
    );
    raw.then(
      () => finish(),
      () => finish(),
    );
    const finish = () => {
      this.#requests--;
      options.signal?.removeEventListener("abort", abort);
      this.signal.removeEventListener("abort", abort);
    };
    return raced(raw, [options.signal, this.signal]);
  }
  async openStream(
    input: S,
    options: RequestOptions = {},
  ): Promise<SecureBridgeStream> {
    this.check();
    if (!this.#peer || this.#state.phase !== "connected")
      throw error("BRIDGE_CLOSED");
    if (options.signal?.aborted) throw error("CANCELLED");
    if (this.#streamCount >= this.#limits.maxStreams)
      throw error("BRIDGE_CAPACITY_EXCEEDED");
    this.#streamCount++;
    const controller = new AbortController(),
      abort = () => controller.abort(error("CANCELLED"));
    options.signal?.addEventListener("abort", abort, { once: true });
    this.signal.addEventListener("abort", abort, { once: true });
    let delivered = false;
    const raw = Promise.resolve().then(() => {
      this.check();
      if (controller.signal.aborted) throw error("CANCELLED");
      return this.#peer!.openStream(input, { signal: controller.signal });
    });
    const wrapped = this.own(
      raw
        .then(
          async (stream) => {
            if (this.signal.aborted || options.signal?.aborted) {
              const cleanup = Promise.all([
                stream.readable.cancel(),
                stream.writable.abort(),
              ]).then(() => {});
              this.ownLate(cleanup);
              try {
                await cleanup;
              } catch {
                void this.closeWith("protocol-error");
              }
              throw error("CANCELLED");
            }
            let wrapper: SecureBridgeStream;
            try {
              wrapper = this.stream(stream);
            } catch {
              void this.closeWith("protocol-error");
              throw error("BRIDGE_PROTOCOL_ERROR");
            }
            delivered = true;
            return wrapper;
          },
          () => {
            throw error("BRIDGE_CLOSED");
          },
        )
        .finally(() => {
          options.signal?.removeEventListener("abort", abort);
          this.signal.removeEventListener("abort", abort);
          if (!delivered) this.#streamCount--;
        }),
    );
    return raced(wrapped, [options.signal, this.signal]);
  }
  stream(raw: AdapterByteStream): SecureBridgeStream {
    if (
      !(raw?.readable instanceof ReadableStream) ||
      !(raw.writable instanceof WritableStream)
    )
      throw error("BRIDGE_PROTOCOL_ERROR");
    const reader = raw.readable.getReader(),
      writer = raw.writable.getWriter(),
      controller = new AbortController();
    let closed = false,
      reading = false,
      writing = false,
      closing: Promise<void> | undefined;
    const close = () => {
      if (closing) return closing;
      closed = true;
      controller.abort(error("BRIDGE_CLOSED"));
      closing = Promise.all([reader.cancel(), writer.abort()]).then(
        () => {
          this.#streamCount--;
          this.#streams.delete(close);
        },
        () => {
          void this.closeWith("protocol-error");
          throw error("BRIDGE_CLOSED");
        },
      );
      this.ownLate(closing);
      return closing;
    };
    this.#streams.add(close);
    const check = () => {
      this.check();
      if (closed) throw error("BRIDGE_CLOSED");
    };
    const abortOperation = (signal?: AbortSignal) => {
      const abort = () => {
        void this.own(close()).catch(() => {});
      };
      signal?.addEventListener("abort", abort, { once: true });
      if (signal?.aborted) abort();
      return () => signal?.removeEventListener("abort", abort);
    };
    return Object.freeze({
      read: async (options: RequestOptions = {}) => {
        check();
        if (options.signal?.aborted) throw error("CANCELLED");
        if (reading) throw error("BRIDGE_CAPACITY_EXCEEDED");
        reading = true;
        const off = abortOperation(options.signal);
        const operation = this.own(
          reader.read().then((item) => {
            if (item.done) {
              void this.own(close()).catch(() => {});
              return null;
            }
            if (
              !(item.value instanceof Uint8Array) ||
              item.value.byteLength > this.#limits.maxFrameBytes ||
              item.value.byteLength > 1048576
            ) {
              void this.closeWith("protocol-error");
              throw error("BRIDGE_PROTOCOL_ERROR");
            }
            return item.value.slice();
          }),
        );
        operation.then(
          () => {
            reading = false;
            off();
          },
          () => {
            reading = false;
            off();
          },
        );
        return raced(operation, [
          options.signal,
          this.signal,
          controller.signal,
        ]);
      },
      write: async (bytes: Uint8Array, options: RequestOptions = {}) => {
        check();
        if (options.signal?.aborted) throw error("CANCELLED");
        if (
          !(bytes instanceof Uint8Array) ||
          bytes.byteLength > this.#limits.maxFrameBytes ||
          bytes.byteLength > 1048576
        )
          throw error("BRIDGE_PROTOCOL_ERROR");
        if (writing) throw error("BRIDGE_CAPACITY_EXCEEDED");
        writing = true;
        const off = abortOperation(options.signal),
          copy = bytes.slice();
        const operation = this.own(
          Promise.resolve().then(() => {
            check();
            return writer.write(copy);
          }),
        );
        operation.then(
          () => {
            writing = false;
            off();
          },
          () => {
            writing = false;
            off();
          },
        );
        await raced(operation, [
          options.signal,
          this.signal,
          controller.signal,
        ]);
      },
      close: () => this.own(close()),
    });
  }
  close(): Promise<BridgeState> {
    return this.closeWith("closed");
  }
  closeWith(reason: NonNullable<BridgeState["reason"]>): Promise<BridgeState> {
    if (this.#cleanup) return this.#cleanup.then(() => this.#state);
    this.#state = Object.freeze({
      phase: "closed",
      reason,
      cleanupPending: true,
    });
    this.#life.abort(error("BRIDGE_CLOSED"));
    if (this.#timer) clearTimeout(this.#timer);
    this.#watch?.stop();
    this.#off();
    const operations = [
      ...this.#pending,
      ...this.#late,
      ...[...this.#streams].map((close) => close()),
      ...(this.#peer ? [this.disposeLate(this.#peer)] : []),
    ];
    const complete = Promise.allSettled(operations).then((results) => {
      const confirmed =
        !this.#cleanupFailed && results.every((r) => r.status === "fulfilled");
      this.#state = Object.freeze({
        ...this.#state,
        cleanupPending: !confirmed,
      });
      if (confirmed) this.host.release();
      return this.#state;
    });
    let timer: ReturnType<typeof setTimeout>;
    this.#cleanup = Promise.race([
      complete,
      new Promise<BridgeState>((resolve) => {
        timer = setTimeout(() => resolve(this.#state), 5000);
      }),
    ]).finally(() => clearTimeout(timer));
    return this.#cleanup;
  }
}
export async function connectControlledBridge<Req, Res, S>(
  host: Host,
  product: string,
  device: string,
  id: string,
  adapter: TrustedBridgeAdapter<Req, Res, S>,
  options: BridgeConnectOptions,
): Promise<SecureBridge<Req, Res, S>> {
  const timeout = options.timeoutMs ?? 60000;
  if (
    !Number.isSafeInteger(timeout) ||
    timeout < 1000 ||
    timeout > 120000 ||
    !adapter ||
    typeof adapter.connect !== "function" ||
    typeof adapter.protocol !== "string"
  ) {
    host.release();
    throw new CyberGridError("INVALID_INPUT", undefined, { stage: "bridge" });
  }
  const controller = new AbortController(),
    cancel = () => controller.abort(error("CANCELLED"));
  options.signal?.addEventListener("abort", cancel, { once: true });
  if (options.signal?.aborted) cancel();
  const start = performance.now(),
    timer = setTimeout(() => controller.abort(error("TIMEOUT")), timeout);
  let managed: Managed<Req, Res, S> | undefined;
  let dispatched = false;
  try {
    const snapshot = await raced(
        host.directory(product, { signal: controller.signal }),
        [controller.signal],
      ),
      found = selected(snapshot, device, id),
      identity = host.identity();
    if (!identity.userId || identity.epoch !== snapshot.epoch)
      throw error("BRIDGE_UNAVAILABLE");
    if (adapter.protocol !== found.bridge.protocol)
      throw error("UNSUPPORTED_BRIDGE");
    const target = deepFreeze({
      userId: identity.userId,
      productId: product,
      deviceId: device,
      epoch: snapshot.epoch,
      generation: identity.generation,
      incarnation: found.device.incarnation,
      publishRevision: found.device.publishRevision,
      endpointPolicyVersion: snapshot.endpointPolicyVersion,
      descriptor: found.bridge,
    });
    managed = new Managed(host, target);
    managed.confirm(snapshot);
    managed.check();
    managed.monitor();
    const current = managed;
    dispatched = true;
    const raw = Promise.resolve().then(() => {
      current.check();
      return adapter.connect(
        target,
        Object.freeze({
          signal: current.signal,
          remainingMs: Math.max(0, timeout - (performance.now() - start)),
        }),
      );
    });
    current.ownLate(
      raw.then(
        async (peer) => {
          if (controller.signal.aborted || current.signal.aborted)
            await current.disposeLate(peer);
        },
        () => {},
      ),
    );
    const peer = await raced(raw, [controller.signal, current.signal]);
    await current.install(peer);
    const latest = await raced(
      host.directory(product, { signal: controller.signal }),
      [controller.signal, current.signal],
    );
    current.confirm(latest);
    current.check();
    return Object.freeze({
      request: (input: Req, o?: RequestOptions) => current.request(input, o),
      openStream: (input: S, o?: RequestOptions) =>
        current.openStream(input, o),
      state: () => current.state(),
      close: () => current.close(),
    });
  } catch (e) {
    if (managed)
      void managed.closeWith(
        controller.signal.aborted ? "cancelled" : "target-changed",
      );
    if (!dispatched) host.release();
    if (e instanceof CyberGridError) throw e;
    throw error("BRIDGE_UNAVAILABLE");
  } finally {
    clearTimeout(timer);
    options.signal?.removeEventListener("abort", cancel);
  }
}
