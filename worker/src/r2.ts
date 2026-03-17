import type { Conversation, ConversationMeta } from "./schema";

// Key layout — identical to Rust ConversationStore:
//   conversations/{conv_id}.json
//   users/{user_id}/conversations.json
//   blobs/{blake3_hex}

export async function getConversation(
  r2: R2Bucket,
  id: string,
): Promise<Conversation | null> {
  const obj = await r2.get(`conversations/${id}.json`);
  if (!obj) return null;
  return obj.json<Conversation>();
}

export async function saveConversation(
  r2: R2Bucket,
  conv: Conversation,
): Promise<void> {
  await r2.put(
    `conversations/${conv.id}.json`,
    JSON.stringify(conv),
    { httpMetadata: { contentType: "application/json" } },
  );
  await updateUserIndex(r2, conv);
}

export async function deleteConversation(
  r2: R2Bucket,
  userId: string,
  convId: string,
): Promise<void> {
  await r2.delete(`conversations/${convId}.json`);
  const index = await listConversations(r2, userId);
  const filtered = index.filter((m) => m.id !== convId);
  await r2.put(
    `users/${userId}/conversations.json`,
    JSON.stringify(filtered),
    { httpMetadata: { contentType: "application/json" } },
  );
}

export async function listConversations(
  r2: R2Bucket,
  userId: string,
): Promise<ConversationMeta[]> {
  const obj = await r2.get(`users/${userId}/conversations.json`);
  if (!obj) return [];
  const index = await obj.json<ConversationMeta[]>();
  return index.sort(
    (a, b) => new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime(),
  );
}

async function updateUserIndex(r2: R2Bucket, conv: Conversation): Promise<void> {
  const key = `users/${conv.user_id}/conversations.json`;
  let index = await listConversations(r2, conv.user_id);

  const meta: ConversationMeta = {
    id:         conv.id,
    title:      conv.title,
    updated_at: conv.updated_at,
  };

  const existing = index.findIndex((m) => m.id === conv.id);
  if (existing >= 0) index[existing] = meta;
  else index.push(meta);

  await r2.put(key, JSON.stringify(index), {
    httpMetadata: { contentType: "application/json" },
  });
}

// Resolve a BLAKE3 blob reference
export async function resolveBlob(
  r2: R2Bucket,
  blobKey: string,
): Promise<string | null> {
  const obj = await r2.get(blobKey);
  if (!obj) return null;
  return obj.text();
}
