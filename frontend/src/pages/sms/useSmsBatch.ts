import { useState, useCallback } from 'react'

export function useSmsBatch<T = string | number>() {
  const [batchMode, setBatchMode] = useState(false)
  const [selectedIds, setSelectedIds] = useState<Set<T>>(() => new Set())

  const toggleSelect = useCallback((id: T) => {
    setSelectedIds((prev) => {
      const next = new Set(prev)
      if (next.has(id)) {
        next.delete(id)
      } else {
        next.add(id)
      }
      return next
    })
  }, [])

  const toggleSelectAll = useCallback((allIds: T[]) => {
    setSelectedIds((prev) => {
      const allSelected = allIds.length > 0 && allIds.every((id) => prev.has(id))
      const next = new Set(prev)
      if (allSelected) {
        allIds.forEach((id) => next.delete(id))
      } else {
        allIds.forEach((id) => next.add(id))
      }
      return next
    })
  }, [])

  const isSelected = useCallback((id: T) => selectedIds.has(id), [selectedIds])

  const clearSelection = useCallback(() => {
    setSelectedIds(new Set())
  }, [])

  const exitBatchMode = useCallback(() => {
    setBatchMode(false)
    setSelectedIds(new Set())
  }, [])

  const selectedCount = selectedIds.size
  const hasSelection = selectedCount > 0

  return {
    batchMode,
    setBatchMode,
    selectedIds,
    setSelectedIds,
    toggleSelect,
    toggleSelectAll,
    isSelected,
    clearSelection,
    exitBatchMode,
    selectedCount,
    hasSelection,
  }
}
