export interface BaseSmsMessage {
  id: string | number
  direction: 'incoming' | 'outgoing' | string
  phone_number: string
  content: string
  timestamp: string
  status?: 'pending' | 'sent' | 'failed' | 'received' | 'unknown' | string
  transport?: string | null
  pdu?: string
  device_id?: string
  device_name?: string
  [key: string]: any
}

export interface BaseConversation {
  phone_number: string
  message_count: number
  last_message: BaseSmsMessage
  unread_count?: number
  device_id?: string
  device_name?: string
  [key: string]: any
}

export type DeleteTarget =
  | { type: 'batch' }
  | { type: 'conversation'; phone_number: string; message_count?: number; [key: string]: any }
  | { type: 'message'; message: BaseSmsMessage }
