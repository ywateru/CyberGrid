# Build context is server/ only; no SDK, credentials or provider blobs are copied.
FROM node:22-bookworm-slim AS build
WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci --include=dev
COPY tsconfig.json ./
COPY src ./src
COPY public ./public
COPY release/build.mjs ./release/build.mjs
RUN npm run build && npm prune --omit=dev

FROM node:22-bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends python3 \
    && rm -rf /var/lib/apt/lists/* \
    && mkdir -p /var/run/cybergrid && chown node:node /var/run/cybergrid
WORKDIR /app
COPY --from=build --chown=node:node /app /app
ENV NODE_ENV=production CYBERGRID_BIND=0.0.0.0 PORT=7400 CYBERGRID_OWNER_LOCK=/var/run/cybergrid/lease
USER node
EXPOSE 7400
STOPSIGNAL SIGTERM
HEALTHCHECK --interval=30s --timeout=3s --start-period=10s --retries=3 \
 CMD node -e "require('node:http').get({host:'127.0.0.1',port:process.env.PORT||7400,path:'/healthz'},r=>{r.resume();process.exitCode=r.statusCode===200?0:1}).on('error',()=>process.exitCode=1)"
# Same command/entrypoint as npm start, with Node as PID1 for signal forwarding.
CMD ["node", "--env-file-if-exists=.env", "--import", "tsx", "src/main.ts"]
