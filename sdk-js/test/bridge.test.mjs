import { test } from "node:test";
import assert from "node:assert/strict";
import { createHash, randomBytes } from "node:crypto";
import { inspect } from "node:util";
import { CyberGridClient, CyberGridError } from "../dist/index.js";
const epoch = "12345678-1234-4234-8234-123456789012",
  incarnation = "23456789-1234-4234-8234-123456789012";
const issuer = "https://identity.example/oidc",
  baseUrl = "https://grid.example",
  clientId = "ConsoleSPA",
  resource = baseUrl + "/api";
const policy = (providers = []) => ({
  version: 1,
  policyVersion: createHash("sha256")
    .update(
      JSON.stringify([
        "cybergrid-endpoint-policy-v1",
        [...providers]
          .sort((a, b) => a.id.localeCompare(b.id))
          .map((p) => [p.id, [...p.origins].sort(), [...p.paths].sort()]),
      ]),
    )
    .digest("hex"),
  providers,
});
const config = (p) => ({
  provider: "logto",
  issuer,
  clientId,
  resource,
  scope: "connect",
  authorizationEndpoint: issuer + "/auth",
  tokenEndpoint: issuer + "/token",
  revocationEndpoint: issuer + "/token/revocation",
  jwksUri: issuer + "/jwks",
  coordination: { protocol: "memory-v1", epoch },
  ...(p === null ? {} : { endpointPolicy: p }),
});
const descriptor = (ms = 20000) => ({
  id: "business",
  protocol: "fixture.noise",
  providerId: "quick-tunnel",
  url: "wss://fixture.trycloudflare.com/remote/v1/ws",
  installationId: "install",
  publicKey: randomBytes(32).toString("base64url"),
  addressRevision: 1,
  expiresAt: new Date(Date.now() + ms).toISOString(),
  limits: { maxFrameBytes: 4096, maxStreams: 2, maxPendingRequests: 2 },
});
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const deferred = () => {
  let resolve, reject;
  const promise = new Promise((a, b) => {
    resolve = a;
    reject = b;
  });
  return { promise, resolve, reject };
};
const fixed = (code) => (e) =>
  e instanceof CyberGridError &&
  e.code === code &&
  !inspect(e).includes("private-adapter");
function fixture(
  t,
  { bridge = descriptor(), p = policy(), timeoutMs = 1000 } = {},
) {
  const state = {
    bridge,
    p,
    publishRevision: 1,
    online: true,
    incarnation,
    directoryCalls: 0,
    configCalls: 0,
    fail: false,
    hook: null,
    configHook: null,
    deviceExtra: {},
  };
  const response = (body, status = 200) =>
    new Response(JSON.stringify(body), {
      status,
      headers: { "x-cybergrid-epoch": epoch },
    });
  state.snapshot = () => ({
    version: 1,
    productId: "Core",
    epoch,
    endpointPolicyVersion: state.p?.policyVersion ?? policy().policyVersion,
    revision: "v1." + epoch + "." + "a".repeat(64) + "." + "b".repeat(64),
    sampledAt: new Date().toISOString(),
    reset: state.directoryCalls === 1,
    changed: state.directoryCalls === 1,
    devices: [
      {
        id: "peer",
        name: "Peer",
        online: state.online,
        lastSeen: new Date().toISOString(),
        ports: [],
        tunnelUrl: null,
        certificateFingerprint: null,
        incarnation: state.incarnation,
        publishRevision: state.publishRevision,
        bridges: state.bridge ? [state.bridge] : [],
        ...state.deviceExtra,
      },
    ],
  });
  const original = globalThis.fetch;
  globalThis.fetch = async (url, init) => {
    const u = new URL(url);
    assert.equal(init.credentials, "omit");
    assert.equal(init.redirect, "error");
    if (u.pathname.endsWith("/console/auth/config")) {
      state.configCalls++;
      assert.equal(u.searchParams.get("clientId"), clientId);
      if (state.configHook) return state.configHook(u, init, response);
      return response(config(state.p));
    }
    if (u.pathname.endsWith("/console/auth/exchange"))
      return response({
        purpose: "console",
        sessionKey: "a".repeat(43),
        user: { id: "owner" },
        expiresAt: new Date(Date.now() + 3600000).toISOString(),
        coordinatorEpoch: epoch,
      });
    if (u.pathname.endsWith("/console/session")) return response({ ok: true });
    if (u.pathname.endsWith("/console/directory")) {
      state.directoryCalls++;
      if (state.hook) return state.hook(u, init, response);
      if (state.fail) throw Error("private-adapter-directory");
      return response(state.snapshot());
    }
    throw Error("unexpected path");
  };
  const client = new CyberGridClient({
    baseUrl,
    issuer,
    clientId,
    resource,
    getAccessToken: async () => "host-access",
    timeoutMs,
  });
  t.after(async () => {
    await client.disconnect();
    globalThis.fetch = original;
  });
  return { client, state };
}
function adapter(options = {}) {
  const ended = deferred(),
    stats = {
      calls: 0,
      closed: 0,
      requests: 0,
      streams: 0,
      target: null,
      signal: null,
    };
  const a = {
    protocol: "fixture.noise",
    async connect(target, { signal }) {
      stats.calls++;
      stats.target = target;
      stats.signal = signal;
      if (options.connect) return options.connect(target, signal);
      return {
        encrypted: true,
        identity: {
          protocol: target.descriptor.protocol,
          publicKey: target.descriptor.publicKey,
          installationId: target.descriptor.installationId,
        },
        limits: target.descriptor.limits,
        closed: ended.promise,
        request: async (input, context) => {
          stats.requests++;
          return options.request ? options.request(input, context) : input;
        },
        openStream: async (input, context) => {
          stats.streams++;
          return options.stream
            ? options.stream(input, context)
            : {
                readable: new ReadableStream({
                  start(c) {
                    c.enqueue(new Uint8Array([1, 2, 3]));
                  },
                }),
                writable: new WritableStream(),
              };
        },
        close: async () => {
          stats.closed++;
          if (options.close) await options.close();
          ended.resolve();
        },
        ...options.patch,
      };
    },
  };
  return { a, stats, ended };
}
async function connected(t, options = {}, adapterOptions = {}) {
  const f = fixture(t, options);
  await f.client.connect();
  const a = adapter(adapterOptions),
    bridge = await f.client.connectBridge("Core", "peer", "business", a.a);
  return { ...f, ...a, bridge };
}

test("trusted provider hash, canonical endpoint grammar and complete console snapshot are strict", async (t) => {
  const p = policy([
    {
      id: "owned",
      origins: ["https://owned.example:8443"],
      paths: ["/", "/exact"],
    },
  ]);
  const f = fixture(t, { p });
  await f.client.connect();
  const s = await f.client.directory("Core");
  assert.ok(Object.isFrozen(s) && Object.isFrozen(s.devices[0].bridges[0]));
  for (const patch of [
    { url: "wss://fixture.trycloudflare.com/a/../remote/v1/ws" },
    { url: "wss://fixture.trycloudflare.com:443/remote/v1/ws" },
    { url: "wss://fixture.trycloudflare.com/%2e" },
    { url: "wss://fixture.trycloudflare.com/a?x=1" },
    { url: "wss://fixture.trycloudflare.com/a#x" },
    { url: "wss://a".padEnd(72, "a") + ".trycloudflare.com/a" },
    { publicKey: "x".repeat(43) },
    { limits: { ...f.state.bridge.limits, maxStreams: 33 } },
  ]) {
    const old = f.state.bridge;
    f.state.bridge = { ...old, ...patch };
    await assert.rejects(f.client.directory("Core"), fixed("INVALID_RESPONSE"));
    f.state.bridge = old;
  }
  const original = f.state.bridge;
  f.state.bridge = {
    ...original,
    providerId: "owned",
    url: "wss://owned.example:8443/exact",
  };
  assert.equal(
    (await f.client.directory("Core")).devices[0].bridges[0].url,
    f.state.bridge.url,
  );
  f.state.bridge = { ...f.state.bridge, url: "wss://owned.example/exact" };
  await assert.rejects(f.client.directory("Core"), fixed("INVALID_RESPONSE"));
  f.state.bridge = original;
  f.state.deviceExtra = { iroh: null };
  await assert.rejects(f.client.directory("Core"), fixed("INVALID_RESPONSE"));
  f.state.deviceExtra = {};
  for (const product of ["", "Core/other", "a".repeat(22)])
    await assert.rejects(f.client.directory(product), fixed("INVALID_INPUT"));
  await assert.rejects(
    f.client.directory("Core", { cursor: "x".repeat(513) }),
    fixed("INVALID_INPUT"),
  );
});
test("legacy config cannot authorize new directory or bridge; bad provider hash never falls back", async (t) => {
  const f = fixture(t, { p: null });
  await f.client.connect();
  await assert.rejects(
    f.client.directory("Core"),
    fixed("CONFIGURATION_MISMATCH"),
  );
  await f.client.disconnect();
  f.state.p = { ...policy(), policyVersion: "0".repeat(64) };
  await assert.rejects(f.client.connect(), fixed("INVALID_RESPONSE"));
});
test("watch reads immediately, serializes response handling, isolates observers and stops late delivery", async (t) => {
  const f = fixture(t, { timeoutMs: 3000 }),
    pending = deferred(),
    events = [];
  await f.client.connect();
  f.state.hook = async (_u, _i, json) => {
    await pending.promise;
    return json(f.state.snapshot());
  };
  const watch = f.client.watchDirectory(
    "Core",
    (event) => {
      events.push(event);
      throw Error("observer");
    },
    { intervalMs: 1000 },
  );
  await sleep(30);
  assert.equal(f.state.directoryCalls, 1);
  await sleep(1020);
  assert.equal(
    f.state.directoryCalls,
    1,
    "no overlapping polling while prior request waits",
  );
  watch.stop();
  await watch.done;
  pending.resolve();
  await sleep(20);
  assert.equal(events.length, 0);
  f.state.hook = null;
  let done;
  const delivered = new Promise((r) => (done = r));
  const second = f.client.watchDirectory(
    "Core",
    (event) => {
      events.push(event);
      done();
    },
    { intervalMs: 1000 },
  );
  await delivered;
  assert.ok(events[0].eventRevision > 0);
  second.stop();
  await second.done;
});
test("watch failure emits fixed unknown and disconnect cancels independent watch timers", async (t) => {
  const f = fixture(t);
  await f.client.connect();
  f.state.fail = true;
  const ready = deferred();
  const w = f.client.watchDirectory("Core", (e) => ready.resolve(e));
  const e = await ready.promise;
  assert.equal(e.type, "unknown");
  assert.equal(e.error.code, "NETWORK_ERROR");
  assert.ok(!inspect(e).includes("private-adapter"));
  await f.client.disconnect();
  await w.done;
});
test("directory policy transition closes active channel before pinned bounded reload and never admits arbitrary origins", async (t) => {
  const f = await connected(t);
  const next = policy([
    {
      id: "owned",
      origins: ["https://owned.example"],
      paths: ["/remote/v1/ws"],
    },
  ]);
  f.state.p = next;
  const hold = deferred();
  f.state.configHook = async (_u, _i, json) => {
    await hold.promise;
    return json(config(next));
  };
  const read = f.client.directory("Core");
  await sleep(10);
  assert.equal(f.bridge.state().phase, "closed");
  await assert.rejects(
    f.bridge.request("must-not-run"),
    fixed("BRIDGE_CLOSED"),
  );
  hold.resolve();
  assert.equal((await read).endpointPolicyVersion, next.policyVersion);
  assert.equal(f.state.configCalls, 2);
  assert.equal(f.stats.closed, 1);
});
test("handshake return rechecks current installation, key, address and incarnation", async (t) => {
  const f = fixture(t);
  await f.client.connect();
  const peer = deferred(),
    a = adapter({
      connect: async (target) => {
        await peer.promise;
        return {
          encrypted: true,
          identity: {
            protocol: target.descriptor.protocol,
            publicKey: target.descriptor.publicKey,
            installationId: target.descriptor.installationId,
          },
          limits: target.descriptor.limits,
          closed: new Promise(() => {}),
          request: async (x) => x,
          openStream: async () => {},
          close: async () => {
            a.stats.closed++;
          },
        };
      },
    });
  const work = f.client.connectBridge("Core", "peer", "business", a.a);
  await sleep(10);
  f.state.incarnation = "33456789-1234-4234-8234-123456789012";
  peer.resolve();
  await assert.rejects(work, (e) =>
    ["BRIDGE_CLOSED", "BRIDGE_UNAVAILABLE"].includes(e.code),
  );
  await sleep(5);
  assert.equal(a.stats.closed, 1);
});
test("wrapper never returns raw peer and constrains pending requests including canceled uncooperative operations", async (t) => {
  const waits = [],
    f = await connected(
      t,
      {},
      {
        request: async () => {
          const p = deferred();
          waits.push(p);
          return p.promise;
        },
      },
    );
  assert.deepEqual(Object.keys(f.bridge).sort(), [
    "close",
    "openStream",
    "request",
    "state",
  ]);
  assert.ok(Object.isFrozen(f.stats.target));
  assert.equal(Object.hasOwn(f.stats.target, "sessionKey"), false);
  const one = new AbortController(),
    two = new AbortController();
  const x = f.bridge.request("x", { signal: one.signal }),
    y = f.bridge.request("y", { signal: two.signal });
  await sleep(0);
  one.abort();
  two.abort();
  await assert.rejects(x, fixed("CANCELLED"));
  await assert.rejects(y, fixed("CANCELLED"));
  await assert.rejects(
    f.bridge.request("overflow"),
    fixed("BRIDGE_CAPACITY_EXCEEDED"),
  );
  assert.equal(f.stats.requests, 2);
  waits.forEach((p) => p.resolve("settled"));
  await sleep(0);
  const z = f.bridge.request("later");
  await sleep(0);
  waits[2].resolve("ok");
  assert.equal(await z, "ok");
  await f.bridge.close();
});
test("stream reads and writes have bounds/backpressure and caller cancellation permanently closes that stream", async (t) => {
  const read = deferred(),
    write = deferred();
  let cancelled = 0,
    aborted = 0;
  const f = await connected(
    t,
    {},
    {
      stream: async () => ({
        readable: new ReadableStream({
          pull: () => read.promise,
          cancel() {
            cancelled++;
          },
        }),
        writable: new WritableStream({
          write: () => write.promise,
          abort() {
            aborted++;
          },
        }),
      }),
    },
  );
  const stream = await f.bridge.openStream(null);
  assert.deepEqual(Object.keys(stream).sort(), ["close", "read", "write"]);
  await assert.rejects(
    stream.write(new Uint8Array(4097)),
    fixed("BRIDGE_PROTOCOL_ERROR"),
  );
  const c = new AbortController(),
    r = stream.read({ signal: c.signal });
  await assert.rejects(stream.read(), fixed("BRIDGE_CAPACITY_EXCEEDED"));
  c.abort();
  await assert.rejects(r, (e) =>
    ["CANCELLED", "BRIDGE_CLOSED"].includes(e.code),
  );
  await assert.rejects(stream.read(), fixed("BRIDGE_CLOSED"));
  await sleep(0);
  assert.equal(cancelled, 1);
  assert.equal(aborted, 1);
  read.resolve();
  await f.bridge.close();
});
test("independent monotonic expiry closes even without another directory read or wall-clock progress", async (t) => {
  const f = await connected(t, { bridge: descriptor(170) });
  const now = Date.now;
  Date.now = () => now() - 86400000;
  t.after(() => (Date.now = now));
  await sleep(210);
  assert.equal(f.bridge.state().phase, "closed");
  assert.equal(f.bridge.state().reason, "expired");
  assert.equal(f.stats.closed, 1);
  await assert.rejects(f.bridge.request("expired"), fixed("BRIDGE_CLOSED"));
});
test("heartbeat same expiry never extends lease; only newer ordered publication may renew unchanged address", async (t) => {
  const f = await connected(t, { bridge: descriptor(2500) });
  await sleep(1200);
  f.state.publishRevision = 2;
  f.state.bridge = {
    ...f.state.bridge,
    expiresAt: new Date(Date.now() + 3400).toISOString(),
  };
  await sleep(1600);
  assert.equal(
    f.bridge.state().phase,
    "connected",
    "watch confirmation extends same address with newer pub",
  );
  await f.bridge.close();
});
test("identity and limits returned by trusted adapter are still compared, with sanitized failures", async (t) => {
  const f = fixture(t);
  await f.client.connect();
  for (const patch of [
    { encrypted: false },
    {
      identity: {
        protocol: "fixture.noise",
        publicKey: "bad",
        installationId: "install",
      },
    },
    { limits: { maxFrameBytes: 99999, maxStreams: 2, maxPendingRequests: 2 } },
  ]) {
    const a = adapter({ patch });
    await assert.rejects(
      f.client.connectBridge("Core", "peer", "business", a.a),
      (e) =>
        ["BRIDGE_IDENTITY_MISMATCH", "BRIDGE_PROTOCOL_ERROR"].includes(e.code),
    );
    await sleep(0);
    assert.equal(a.stats.closed, 1);
  }
  const a = adapter({
    request: async () => {
      throw Error("private-adapter-original-error");
    },
  });
  const bridge = await f.client.connectBridge("Core", "peer", "business", a.a);
  await assert.rejects(bridge.request("x"), fixed("BRIDGE_CLOSED"));
  assert.equal(
    (await bridge.close()).cleanupPending,
    false,
    "ordinary request rejection is not cleanup failure",
  );
});
test("caller cancel rejects promptly, owns late peer close, and total connect timeout includes handshake", async (t) => {
  const f = fixture(t);
  await f.client.connect();
  const peer = deferred(),
    close = deferred(),
    a = adapter({
      connect: async (target) => {
        await peer.promise;
        return {
          encrypted: true,
          identity: {
            protocol: target.descriptor.protocol,
            publicKey: target.descriptor.publicKey,
            installationId: target.descriptor.installationId,
          },
          limits: target.descriptor.limits,
          closed: new Promise(() => {}),
          request: async (x) => x,
          openStream: async () => {},
          close: async () => {
            a.stats.closed++;
            await close.promise;
          },
        };
      },
    });
  const c = new AbortController(),
    work = f.client.connectBridge("Core", "peer", "business", a.a, {
      signal: c.signal,
    });
  await sleep(10);
  c.abort();
  await assert.rejects(work, fixed("CANCELLED"));
  peer.resolve();
  await sleep(10);
  assert.equal(a.stats.closed, 1);
  assert.equal(a.stats.signal.aborted, true);
  close.resolve();
  const b = adapter({ connect: async () => new Promise(() => {}) });
  const begin = performance.now();
  await assert.rejects(
    f.client.connectBridge("Core", "peer", "business", b.a, {
      timeoutMs: 1000,
    }),
    fixed("TIMEOUT"),
  );
  assert.ok(performance.now() - begin < 1600);
});
test("close timeout reports cleanupPending and gate remains closed until uncooperative peer settles", async (t) => {
  const closing = deferred(),
    f = await connected(t, {}, { close: () => closing.promise });
  const work = f.bridge.close();
  assert.equal(f.bridge.state().phase, "closed");
  await assert.rejects(f.bridge.request("x"), fixed("BRIDGE_CLOSED"));
  assert.equal((await work).cleanupPending, true);
  closing.resolve();
  await sleep(0);
  assert.equal(f.bridge.state().cleanupPending, false);
  assert.equal((await f.bridge.close()).cleanupPending, false);
});
test("closing streams retain capacity until underlying cancellation settles, and invalid frames close whole channel", async (t) => {
  const cancel = deferred();
  let enqueuer;
  const f = await connected(
    t,
    {},
    {
      stream: async () => ({
        readable: new ReadableStream({
          start(c) {
            enqueuer = c;
          },
          cancel: () => cancel.promise,
        }),
        writable: new WritableStream(),
      }),
    },
  );
  const one = await f.bridge.openStream(null),
    two = await f.bridge.openStream(null);
  const c1 = one.close(),
    c2 = two.close();
  await assert.rejects(
    f.bridge.openStream(null),
    fixed("BRIDGE_CAPACITY_EXCEEDED"),
  );
  cancel.resolve();
  await Promise.all([c1, c2]);
  const third = await f.bridge.openStream(null);
  enqueuer.enqueue(new Uint8Array(4097));
  await assert.rejects(third.read(), (e) =>
    ["BRIDGE_PROTOCOL_ERROR", "BRIDGE_CLOSED"].includes(e.code),
  );
  assert.equal(f.bridge.state().phase, "closed");
});
test("host denial synchronously gates existing channels and rejects a replacement until host changes it", async (t) => {
  const f = await connected(t);
  f.client.setHostBusinessAuthorization("denied");
  assert.equal(f.bridge.state().phase, "closed");
  await assert.rejects(
    f.client.connectBridge("Core", "peer", "business", f.a),
    fixed("FORBIDDEN"),
  );
  await assert.rejects(f.bridge.request("no"), fixed("BRIDGE_CLOSED"));
});
test("same address expiry without newer publication cannot renew original monotonic deadline", async (t) => {
  const f = await connected(t, { bridge: descriptor(2300) });
  await sleep(1100);
  f.state.bridge = {
    ...f.state.bridge,
    expiresAt: new Date(Date.now() + 20000).toISOString(),
  };
  await sleep(1350);
  assert.equal(f.bridge.state().phase, "closed");
  assert.equal(f.bridge.state().reason, "expired");
});
test("policy reload is single flight across products and does not accept a stale directory hash after reload", async (t) => {
  const f = fixture(t);
  await f.client.connect();
  const next = policy([
    { id: "owned", origins: ["https://owned.example"], paths: ["/"] },
  ]);
  f.state.p = next;
  const hold = deferred();
  f.state.configHook = async (_u, _i, json) => {
    await hold.promise;
    return json(config(next));
  };
  const a = f.client.directory("Core"),
    b = f.client.directory("Core");
  await sleep(10);
  assert.equal(f.state.configCalls, 2);
  hold.resolve();
  assert.equal((await a).endpointPolicyVersion, next.policyVersion);
  assert.equal((await b).endpointPolicyVersion, next.policyVersion);
});
test("a previously valid old-policy directory cannot create a bridge while trusted policy reload is pending", async (t) => {
  const f = fixture(t);
  await f.client.connect();
  const old = f.state.p,
    next = policy([
      { id: "owned", origins: ["https://owned.example"], paths: ["/"] },
    ]);
  f.state.p = next;
  const hold = deferred();
  f.state.configHook = async (_u, _i, json) => {
    await hold.promise;
    return json(config(next));
  };
  const pending = f.client.directory("Core");
  await sleep(10);
  f.state.p = old;
  const a = adapter(),
    connection = f.client.connectBridge("Core", "peer", "business", a.a);
  await sleep(20);
  assert.equal(a.stats.calls, 0);
  hold.resolve();
  await pending;
  await assert.rejects(connection, fixed("CONFIGURATION_MISMATCH"));
  assert.equal(a.stats.calls, 0);
});
test("late canceled stream cleanup failure stays owned and cannot be reported as confirmed", async (t) => {
  const open = deferred(),
    f = await connected(t, {}, { stream: () => open.promise });
  const c = new AbortController(),
    pending = f.bridge.openStream(null, { signal: c.signal });
  await sleep(0);
  c.abort();
  await assert.rejects(pending, fixed("CANCELLED"));
  open.resolve({
    readable: new ReadableStream({
      cancel() {
        return Promise.reject(Error("private-adapter-close-failure"));
      },
    }),
    writable: new WritableStream(),
  });
  await sleep(20);
  assert.equal(f.bridge.state().phase, "closed");
  assert.equal(f.bridge.state().cleanupPending, true);
  assert.equal(f.stats.closed, 1);
  await assert.rejects(f.bridge.request("no"), fixed("BRIDGE_CLOSED"));
});
test("cancel before adapter request dispatch never starts a business write", async (t) => {
  const f = await connected(t),
    c = new AbortController();
  const pending = f.bridge.request("must not dispatch", { signal: c.signal });
  c.abort();
  await assert.rejects(pending, fixed("CANCELLED"));
  await sleep(0);
  assert.equal(f.stats.requests, 0);
  await f.bridge.close();
});

test("trusted product removal during pinned reload rejects the selected product without moving or reopening it", async (t) => {
  const f = fixture(t);
  f.state.configHook = (_u, _i, json) => json({
    ...config(f.state.p), policyVersion: "1".repeat(64),
    products: [{ id: "Core", name: "Core" }],
  });
  await f.client.connect();
  const a = adapter(), old = await f.client.connectBridge("Core", "peer", "business", a.a);
  f.state.p = policy([{ id: "owned", origins: ["https://owned.example"], paths: ["/"] }]);
  f.state.configHook = (_u, _i, json) => json({
    ...config(f.state.p), policyVersion: "2".repeat(64),
    products: [{ id: "Other", name: "Other" }],
  });
  await assert.rejects(f.client.directory("Core"), fixed("CONFIGURATION_MISMATCH"));
  assert.equal(old.state().phase, "closed");
  const requests = f.state.directoryCalls, replacement = adapter();
  await assert.rejects(f.client.directory("Core"), fixed("CONFIGURATION_MISMATCH"));
  await assert.rejects(f.client.devices("Core"), fixed("CONFIGURATION_MISMATCH"));
  await assert.rejects(f.client.connectBridge("Core", "peer", "business", replacement.a), fixed("CONFIGURATION_MISMATCH"));
  assert.equal(f.state.directoryCalls, requests, "removed selection must fail before new I/O");
  assert.equal(replacement.stats.calls, 0);
  await assert.rejects(old.request("write-after-removal"), fixed("BRIDGE_CLOSED"));
  assert.equal(a.stats.requests, 0);
});

test("initial trusted product selection and in-flight old directory recheck the current allowed products", async (t) => {
  const f = fixture(t), held = deferred(), entered = deferred();
  f.state.configHook = (_u, _i, json) => json({
    ...config(f.state.p), policyVersion: "1".repeat(64),
    products: [{ id: "Other", name: "Other" }],
  });
  await f.client.connect();
  await assert.rejects(f.client.directory("Core"), fixed("CONFIGURATION_MISMATCH"));
  assert.equal(f.state.directoryCalls, 0);
  await f.client.disconnect();
  f.state.configHook = (_u, _i, json) => json({
    ...config(f.state.p), policyVersion: "2".repeat(64),
    products: [{ id: "Core", name: "Core" }],
  });
  await f.client.connect();
  f.state.hook = async (_u, _i, json) => {
    const value = f.state.snapshot(); entered.resolve(); await held.promise; return json(value);
  };
  const old = f.client.directory("Core");
  await entered.promise;
  f.state.hook = null;
  f.state.p = policy([{ id: "owned", origins: ["https://owned.example"], paths: ["/"] }]);
  f.state.configHook = (_u, _i, json) => json({
    ...config(f.state.p), policyVersion: "3".repeat(64),
    products: [{ id: "Other", name: "Other" }],
  });
  await assert.rejects(f.client.directory("Core"), fixed("CONFIGURATION_MISMATCH"));
  held.resolve();
  await assert.rejects(old, fixed("CONFIGURATION_MISMATCH"));
});

for (const [label, code, rejected] of [
  ["malformed console projection", "INVALID_RESPONSE", (f, json) => json({ ...f.state.snapshot(), devices: [{ ...f.state.snapshot().devices[0], iroh: null }] })],
  ["permission denial", "FORBIDDEN", (_f, json) => json({ error: { code: "FORBIDDEN" } }, 403)],
  ["typed directory capacity", "DIRECTORY_TOO_LARGE", (_f, json) => json({ error: { code: "DIRECTORY_TOO_LARGE" } }, 413)],
  ["oversized raw directory", "INVALID_RESPONSE", () => new Response('"' + "x".repeat(2 * 1024 * 1024) + '"', { headers: { "x-cybergrid-epoch": epoch } })],
]) test(`complete directory ${label} closes the business gate before state observers and unknown callbacks`, async (t) => {
  const f = await connected(t), event = deferred(), observerPhases = [];
  const off = f.client.subscribe(s => { if (s.reason === code.toLowerCase().replaceAll("_", "-")) observerPhases.push(f.bridge.state().phase); });
  t.after(off);
  f.state.hook = (_u, _i, json) => rejected(f, json);
  const watch = f.client.watchDirectory("Core", e => {
    if (e.type === "unknown") {
      const phase = f.bridge.state().phase;
      // Attempt the write from inside the notification, not a later polling turn.
      const write = f.bridge.request("must-not-dispatch-after-unknown");
      event.resolve({ e, phase, write });
    }
  });
  t.after(() => watch.stop());
  const result = await event.promise;
  assert.equal(result.e.error.code, code);
  assert.equal(result.phase, "closed");
  await assert.rejects(result.write, fixed("BRIDGE_CLOSED"));
  assert.deepEqual(observerPhases, ["closed"]);
  assert.equal(f.stats.requests, 0);
  assert.equal(f.client.snapshot().coordination, "unavailable");
  watch.stop(); await watch.done;
});

test("local invalid query and explicit canceled directory do not invalidate a valid business gate", async (t) => {
  const f = await connected(t);
  await assert.rejects(f.client.directory("invalid/product"), fixed("INVALID_INPUT"));
  const canceled = new AbortController(); canceled.abort();
  await assert.rejects(f.client.directory("Core", { signal: canceled.signal }), fixed("CANCELLED"));
  assert.equal(f.bridge.state().phase, "connected");
  assert.equal(await f.bridge.request("still-valid"), "still-valid");
  assert.equal(f.stats.requests, 1);
});

test("same-policy session recovery revalidates products before replaying an old directory query", async (t) => {
  const f = fixture(t);
  let products = [{ id: "Core", name: "Core" }];
  f.state.configHook = (_u, _i, json) => json({
    ...config(f.state.p), policyVersion: "1".repeat(64), products,
  });
  await f.client.connect();
  const a = adapter(), channel = await f.client.connectBridge("Core", "peer", "business", a.a);
  products = [{ id: "Other", name: "Other" }];
  let calls = 0;
  f.state.hook = (_u, _i, json) => {
    calls++;
    return calls === 1 ? json({ error: { code: "INVALID_SESSION" } }, 401) : json(f.state.snapshot());
  };
  await assert.rejects(f.client.directory("Core"), fixed("CONFIGURATION_MISMATCH"));
  assert.equal(calls, 1, "recovery must not replay a product already removed by trusted config");
  assert.equal(channel.state().phase, "closed");
  assert.equal(a.stats.requests, 0);
  assert.equal(f.state.configCalls, 2);
});
