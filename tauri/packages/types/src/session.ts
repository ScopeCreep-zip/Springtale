/**
 * Matches: springtale-store/src/schema/bot.rs — SessionRow
 */
export interface Session {
  user_id: string;
  channel_id: string;
  last_bot_message: string | null;
  pending_command: string | null;
  state_data: string;
  created_at: string; // ISO 8601
  updated_at: string; // ISO 8601
}
