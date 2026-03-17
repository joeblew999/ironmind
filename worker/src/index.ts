import { Hono } from "hono";
import { zValidator } from "@hono/zod-validator";
import { cors } from "hono/cors";
import { ChatRequestSchema } from "./schema";
import { getConversation, listConversations, deleteConversation } from "./r2";
import { validateToken, isLocalRequest } from "./auth";

// ── Env bindings ─────────────────────────────────────────────────────────────
// Defined in wrangler.toml + secrets

export interface Env {
  IRONMIND_R2:            R2Bucket;
  IRONMIND_UPSTREAM_URL:  string;   // Mac Mini Tailscale URL e.g. http://100.x.x.x:3000
  IRONMIND_SHARED_SECRET: string;   // Shared secret for worker→Mac Mini auth
  IRONMIND_ENV:           string;
}

const app = new Hono<{ Bindings: Env }>();

// ── CORS ─────────────────────────────────────────────────────────────────────
app.use("*", cors({ origin: "*", allowMethods: ["GET", "POST", "DELETE", "OPTIONS"] }));

// ── Health ────────────────────────────────────────────────────────────────────
app.get("/health", (c) =>
  c.json({ ok: true, service: "ironmind-worker", env: c.env.IRONMIND_ENV }),
);

// ── Auth middleware (all /api/* routes) ──────────────────────────────────────
app.use("/api/*", async (c, next) => {
  // Skip auth for local/Tailscale requests
  if (isLocalRequest(c.req.raw)) return next();

  const user = await validateToken(
    c.env.IRONMIND_R2,
    c.req.header("Authorization") ?? null,
  );
  if (!user) return c.json({ error: "Unauthorized" }, 401);

  // Attach user to context for downstream handlers
  c.set("userId" as never, user.id);
  return next();
});

// ── POST /api/chat — SSE proxy to Mac Mini ───────────────────────────────────
// The worker streams the SSE response straight through from the Mac Mini.
// This means:
//   Browser → CF Worker (edge, global) → Tailscale → Mac Mini (inference)
// R2 writes happen on the Mac Mini side; the worker just proxies the stream.

app.post(
  "/api/chat",
  zValidator("json", ChatRequestSchema),
  async (c) => {
    const body = c.req.valid("json");
    const upstream = c.env.IRONMIND_UPSTREAM_URL;

    // Forward to Mac Mini with shared secret header
    const upstreamRes = await fetch(`${upstream}/api/chat`, {
      method:  "POST",
      headers: {
        "Content-Type":        "application/json",
        "X-Ironmind-Local":    "1",
        "X-Ironmind-Secret":   c.env.IRONMIND_SHARED_SECRET,
      },
      body: JSON.stringify(body),
    });

    if (!upstreamRes.ok || !upstreamRes.body) {
      return c.json({ error: "Upstream unavailable" }, 502);
    }

    // Stream SSE straight through to browser
    return new Response(upstreamRes.body, {
      headers: {
        "Content-Type":  "text/event-stream",
        "Cache-Control": "no-cache",
        "Connection":    "keep-alive",
        "X-Accel-Buffering": "no",
      },
    });
  },
);

// ── GET /api/conversations ────────────────────────────────────────────────────
app.get("/api/conversations", async (c) => {
  const userId = c.req.query("user_id") ?? "default";
  const list   = await listConversations(c.env.IRONMIND_R2, userId);
  return c.json(list);
});

// ── GET /api/conversations/:id ────────────────────────────────────────────────
app.get("/api/conversations/:id", async (c) => {
  const conv = await getConversation(c.env.IRONMIND_R2, c.req.param("id"));
  if (!conv) return c.json({ error: "Not found" }, 404);
  return c.json(conv);
});

// ── DELETE /api/conversations/:id ────────────────────────────────────────────
app.delete("/api/conversations/:id", async (c) => {
  const userId = c.req.query("user_id") ?? "default";
  await deleteConversation(c.env.IRONMIND_R2, userId, c.req.param("id"));
  return new Response(null, { status: 204 });
});

// ── Serve static UI from R2 (fallback) ───────────────────────────────────────
// index.html is uploaded to R2 under static/index.html during deploy.
// For production the UI can also be on CF Pages — this is the fallback.
app.get("*", async (c) => {
  const path = c.req.path === "/" ? "static/index.html" : `static${c.req.path}`;
  const obj  = await c.env.IRONMIND_R2.get(path);
  if (!obj) return c.notFound();

  const contentType =
    path.endsWith(".html") ? "text/html" :
    path.endsWith(".js")   ? "application/javascript" :
    path.endsWith(".css")  ? "text/css" :
    "application/octet-stream";

  return new Response(obj.body, {
    headers: { "Content-Type": contentType, "Cache-Control": "public, max-age=300" },
  });
});

export default app;
