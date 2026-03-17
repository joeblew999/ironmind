// Auth mirrors ironmind-auth Rust crate:
// Token format: "{user_id}:{32-char-secret}"
// Stored hash: BLAKE3(token) in users/{user_id}/profile.json

interface UserProfile {
  id:          string;
  name:        string;
  token_hash:  string;
  created_at:  string;
}

export async function validateToken(
  r2: R2Bucket,
  authHeader: string | null,
): Promise<UserProfile | null> {
  if (!authHeader?.startsWith("Bearer ")) return null;

  const token  = authHeader.slice(7);
  const userId = token.split(":")[0];
  if (!userId) return null;

  const obj = await r2.get(`users/${userId}/profile.json`);
  if (!obj) return null;

  const profile = await obj.json<UserProfile>();

  // BLAKE3 hash the token and compare
  const encoder    = new TextEncoder();
  const tokenBytes = encoder.encode(token);
  const hashBuffer = await crypto.subtle.digest("SHA-256", tokenBytes);
  // NOTE: We use SHA-256 here because BLAKE3 isn't in WebCrypto.
  // The Rust side must also use SHA-256 for the worker auth path.
  // BLAKE3 is still used for R2 blob keys (content addressing).
  const hashHex = Array.from(new Uint8Array(hashBuffer))
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");

  if (profile.token_hash !== hashHex) return null;
  return profile;
}

// Allow local LAN access without a token (Mac Mini → worker loopback)
export function isLocalRequest(request: Request): boolean {
  const cf = request.cf as { asOrganization?: string } | undefined;
  return cf?.asOrganization === "TAILSCALE" ||
    request.headers.get("X-Ironmind-Local") === "1";
}
