import { z } from "zod";

// ── Matches ironmind_r2::model ───────────────────────────────────────────────

export const MessageRoleSchema = z.enum(["user", "assistant", "system"]);

export const ToolCallRecordSchema = z.object({
  name:     z.string(),
  args:     z.unknown(),
  result:   z.string(),
  blob_key: z.string().optional(),
});

export const MessageSchema = z.object({
  id:         z.string(),
  role:       MessageRoleSchema,
  content:    z.string(),
  tool_calls: z.array(ToolCallRecordSchema).default([]),
  created_at: z.string().datetime(),
});

export const ConversationSchema = z.object({
  id:         z.string().uuid(),
  user_id:    z.string(),
  title:      z.string(),
  messages:   z.array(MessageSchema),
  created_at: z.string().datetime(),
  updated_at: z.string().datetime(),
  mcp_url:    z.string().url(),
});

export const ConversationMetaSchema = z.object({
  id:         z.string().uuid(),
  title:      z.string(),
  updated_at: z.string().datetime(),
});

// ── API request/response ─────────────────────────────────────────────────────

export const ChatRequestSchema = z.object({
  conversation_id: z.string().uuid(),
  message:         z.string().min(1).max(32_000),
  user_id:         z.string().optional(),
  mcp_url:         z.string().url().optional(),
});

export const SseEventSchema = z.discriminatedUnion("type", [
  z.object({ type: z.literal("token"),       text:   z.string() }),
  z.object({ type: z.literal("tool_call"),   name:   z.string(), args: z.unknown() }),
  z.object({ type: z.literal("tool_result"), name:   z.string(), result: z.string() }),
  z.object({ type: z.literal("done"),        rounds: z.number() }),
  z.object({ type: z.literal("error"),       message: z.string() }),
]);

export type ChatRequest       = z.infer<typeof ChatRequestSchema>;
export type Conversation      = z.infer<typeof ConversationSchema>;
export type ConversationMeta  = z.infer<typeof ConversationMetaSchema>;
export type SseEvent          = z.infer<typeof SseEventSchema>;
