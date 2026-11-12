// Actual HTTP/JWK/Redis integration. Adapter lifecycle here is an explicit fixture;
// the external host's actual Noise cryptography is verified separately, privately.
import { test } from "node:test";
import assert from "node:assert/strict";
import { createServer } from "node:http";
import { randomBytes, randomUUID } from "node:crypto";
import { createRequire } from "node:module";
import { CyberGridClient } from "../dist/index.js";
import { redisFixture } from "../../server/test/redis-fixture.ts";
import { createService } from "../../server/src/service.ts";
import { serveAPI } from "../../server/src/http.ts";
import { EndpointPolicy } from "../../server/src/endpoints.ts";
import { ProductPolicy } from "../../server/src/products.ts";
import { LogtoIdentity } from "../../server/src/identity.ts";
import { coordinatorAuthConfiguration } from "../../server/src/deployment-config.ts";
const require = createRequire(
  new URL("../../server/package.json", import.meta.url),
);
const { generateKeyPair, exportJWK, createLocalJWKSet, SignJWT } = await import(
  require.resolve("jose")
);
const delay = (ms) => new Promise((r) => setTimeout(r, ms));
const until = async (predicate) => {
  const deadline = performance.now() + 4000;
  while (!predicate()) {
    assert.ok(performance.now() < deadline, "bounded lifecycle transition");
    await delay(20);
  }
};
test("built ESM + two actual HTTP handlers + ES384 + Redis: native publication to console directory, watch and invalidation", async (t) => {
  const f = await redisFixture(t);
  assert.ok(f, "real Redis is required");
  const stores = [f.store({}, f.prefix), f.store({}, f.prefix)];
  await Promise.all(stores.map((s) => s.ready()));
  const auth = coordinatorAuthConfiguration({
      LOGTO_ISSUER: "https://identity.example/oidc",
      LOGTO_RESOURCE: "https://api.example/api",
      LOGTO_BROWSER_CLIENT_ID: "web",
    }),
    pair = await generateKeyPair("ES384"),
    jwk = {
      ...(await exportJWK(pair.publicKey)),
      kid: "bridge-fixture",
      alg: "ES384",
    };
  const identity = new LogtoIdentity(
      auth,
      ["ES384"],
      createLocalJWKSet({ keys: [jwk] }),
    ),
    products = new ProductPolicy(
      JSON.stringify({
        version: 1,
        products: [
          {
            id: "Core",
            name: "Core",
            nativeClientIds: ["Core", "native"],
            browserClientIds: ["web"],
          },
        ],
      }),
    );
  const p = new EndpointPolicy(
    JSON.stringify({
      version: 1,
      providers: [
        {
          id: "owned",
          origins: ["https://business.example:8443"],
          paths: ["/", "/business"],
        },
      ],
    }),
  );
  const removedProducts = new ProductPolicy(JSON.stringify({ version: 1, products: [
    { id: "Other", name: "Other", nativeClientIds: ["Other"], browserClientIds: ["web"] },
  ] }));
  let configuredProducts = products, directoryProducts = products;
  const frontend = "https://frontend.example";
  let endpoints = p,
    unavailable = false;
  const server = async (index) => {
    const s = createServer((req, res) => {
      if (unavailable && req.url.startsWith("/api/v1/console/directory")) {
        res.writeHead(503, {
          "content-type": "application/json",
          "x-cybergrid-epoch": stores[0].epoch,
        });
        res.end("{}");
        return;
      }
      return serveAPI(
        req,
        res,
        createService({
          store: stores[index],
          identity,
          // Different deployed instances can temporarily serve older product policy.
          directory: req.url.startsWith("/api/v1/console/auth/config") ? configuredProducts : directoryProducts,
          products: req.url.startsWith("/api/v1/console/auth/config") ? configuredProducts : directoryProducts,
          endpointPolicy: endpoints,
          browserClientId: "web",
          browserOrigins: new Set([frontend]),
          authRateLimit: 1000,
        }),
      );
    });
    await new Promise((r) => s.listen(0, "127.0.0.1", r));
    t.after(async () => {
      s.closeAllConnections();
      await new Promise((r) => s.close(r));
    });
    return "http://127.0.0.1:" + s.address().port;
  };
  const left = await server(0),
    right = await server(1),
    original = globalThis.fetch;
  const token = (client, owner = "owner") =>
    new SignJWT({ client_id: client, scope: "connect" })
      .setProtectedHeader({ alg: "ES384", kid: jwk.kid, typ: "at+jwt" })
      .setIssuer(auth.issuer)
      .setAudience(auth.resource)
      .setSubject(owner)
      .setIssuedAt()
      .setExpirationTime("5m")
      .sign(pair.privateKey);
  const call = async (path, key, method = "GET", body) => {
    const response = await original(left + "/api/v1" + path, {
      method,
      headers: {
        "content-type": "application/json",
        "x-cybergrid-epoch": stores[0].epoch,
        ...(key ? { authorization: "Bearer " + key } : {}),
      },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    const data = await response.json();
    assert.equal(response.status, 200, JSON.stringify(data));
    return data;
  };
  const native = await call("/auth/exchange", null, "POST", {
    accessToken: await token("native"),
    appId: "native",
    deviceName: "Native peer",
    registrationKey: randomBytes(32).toString("base64url"),
    registrationRevision: 1,
    requestId: randomUUID(),
    coordinationProtocol: "memory-v1",
    coordinatorEpoch: stores[0].epoch,
  });
  const b = {
    id: "business",
    protocol: "fixture.noise",
    providerId: "owned",
    url: "wss://business.example:8443/business",
    installationId: "installation",
    publicKey: randomBytes(32).toString("base64url"),
    addressRevision: 1,
    expiresAt: new Date(Date.now() + 25000).toISOString(),
    limits: { maxFrameBytes: 4096, maxStreams: 2, maxPendingRequests: 2 },
  };
  let rev = 0;
  const publish = (bridges) =>
    call("/devices/self", native.sessionKey, "PUT", {
      ports: [],
      tunnelUrl: null,
      certificateFingerprint: "b".repeat(64),
      bridges,
      incarnation: native.incarnation,
      publishRevision: ++rev,
    });
  await publish([b]);
  globalThis.fetch = async (url, init) => {
    assert.ok(new URL(url).origin === right);
    const headers = new Headers(init.headers);
    headers.set("origin", frontend);
    const result = await original(url, { ...init, headers });
    assert.equal(result.headers.get("access-control-allow-origin"), frontend);
    return result;
  };
  t.after(() => (globalThis.fetch = original));
  const client = new CyberGridClient({
    baseUrl: right,
    issuer: auth.issuer,
    resource: auth.resource,
    clientId: "web",
    getAccessToken: () => token("web"),
    timeoutMs: 2000,
  });
  t.after(() => client.disconnect());
  await client.connect();
  const first = await client.directory("Core");
  assert.equal(first.devices.length, 1);
  assert.equal(first.devices[0].id, native.deviceId);
  assert.equal(Object.hasOwn(first.devices[0], "iroh"), false);
  assert.equal(first.reset, true);
  const next = await client.directory("Core", { cursor: first.revision });
  assert.equal(next.changed, false);
  assert.equal(next.reset, false);
  await publish([b]);
  const beat = await client.directory("Core", { cursor: first.revision });
  assert.equal(beat.changed, false);
  assert.equal(beat.devices[0].publishRevision, 2);
  let closeCalls = 0;
  const adapter = {
    protocol: "fixture.noise",
    async connect(target) {
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
        openStream: async () => {
          throw Error("unused");
        },
        close: async () => {
          closeCalls++;
        },
      };
    },
  };
  const channel = await client.connectBridge(
    "Core",
    native.deviceId,
    b.id,
    adapter,
  );
  assert.equal(await channel.request("fixture request"), "fixture request");
  await publish([]);
  await until(() => channel.state().phase === "closed");
  assert.equal(closeCalls, 1);
  await assert.rejects(channel.request("after withdrawal"), {
    code: "BRIDGE_CLOSED",
  });
  // Re-advertise the exact binding: withdrawing it did not erase server revision history.
  await publish([b]);
  const reopened = await client.connectBridge(
    "Core",
    native.deviceId,
    b.id,
    adapter,
  );
  unavailable = true;
  await until(() => reopened.state().phase === "closed");
  assert.equal(closeCalls, 2);
  unavailable = false;
  const third = await client.connectBridge(
    "Core",
    native.deviceId,
    b.id,
    adapter,
  );
  endpoints = new EndpointPolicy();
  const filtered = await client.directory("Core");
  assert.equal(filtered.devices[0].bridges.length, 0);
  assert.equal(third.state().phase, "closed");
  assert.equal(closeCalls, 3);
  // A real handler denial must close the existing wrapper before external unknown delivery.
  endpoints = p;
  const fourth = await client.connectBridge("Core", native.deviceId, b.id, adapter);
  directoryProducts = removedProducts;
  const unknown = new Promise((resolve, reject) => {
    const watch = client.watchDirectory("Core", event => {
      if (event.type !== "unknown") return;
      try {
        assert.equal(event.error.code, "FORBIDDEN");
        assert.equal(fourth.state().phase, "closed");
        resolve({ watch, blocked: fourth.request("after-real-handler-denial") });
      } catch (error) { watch.stop(); reject(error); }
    });
  });
  const denied = await unknown;
  denied.watch.stop(); await denied.watch.done;
  await assert.rejects(denied.blocked, { code: "BRIDGE_CLOSED" });
  assert.equal(closeCalls, 4);
  directoryProducts = products;
  const fifth = await client.connectBridge("Core", native.deviceId, b.id, adapter);
  // Config is current but directory still comes from a valid older instance.
  configuredProducts = removedProducts;
  endpoints = new EndpointPolicy(JSON.stringify({ version: 1, providers: [
    { id: "owned", origins: ["https://business.example:8443"], paths: ["/", "/business", "/next"] },
  ] }));
  await assert.rejects(client.directory("Core"), { code: "CONFIGURATION_MISMATCH" });
  assert.equal(fifth.state().phase, "closed");
  await assert.rejects(client.connectBridge("Core", native.deviceId, b.id, adapter), { code: "CONFIGURATION_MISMATCH" });
  assert.equal(closeCalls, 5);
  await client.disconnect();
});
