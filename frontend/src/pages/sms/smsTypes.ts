export interface BaseSmsMessage {
  id: string | number
  direction: string
  phone_number: string
  content: string
  timestamp: string
  status?: string
  transport?: string | null
  pdu?: string
  device_id?: string
  device_name?: string
}

export interface BaseConversation {
  phone_number: string
  message_count: number
  last_message: BaseSmsMessage
  unread_count?: number
  device_id?: string
  device_name?: string
}

export type DeleteTarget =
  | { type: 'batch' }
  | {
      type: 'conversation'
      phone_number?: string
      phoneNumber?: string
      message_count?: number
      messageCount?: number
      conversation?: { phone_number?: string; message_count?: number }
    }
  | { type: 'message'; message: BaseSmsMessage }

