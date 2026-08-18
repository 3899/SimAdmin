/* eslint-disable react-refresh/only-export-components */
import { createContext, useContext, useMemo, type ReactNode } from 'react'
import {
  api as localApi,
  SimAdminCurrentAPI,
  type SimAdminBinaryTransport,
  type SimAdminRequestTransport,
} from '../api/current'

const ApiContext = createContext(localApi)

export function SimAdminApiProvider({
  children,
  transport,
  binaryTransport,
}: {
  children: ReactNode
  transport: SimAdminRequestTransport
  binaryTransport?: SimAdminBinaryTransport
}) {
  const api = useMemo(() => new SimAdminCurrentAPI(transport, binaryTransport ?? null), [binaryTransport, transport])
  return <ApiContext.Provider value={api}>{children}</ApiContext.Provider>
}

export function useSimAdminApi() {
  return useContext(ApiContext)
}
