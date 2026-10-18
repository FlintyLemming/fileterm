import { useCallback, useEffect, useMemo, useRef } from 'react'
import type { ConnectionProfile } from '@fileterm/core'
import { buildHostGroups, resolveActiveHostGroupKey, type HostGroup } from './host-groups'
import type { OrderedTabEntry } from './tab-bar'

export interface HostNavigation {
  groups: HostGroup[]
  activeGroupKey: string
  activeEntries: OrderedTabEntry[]
  onSelectGroup(groupKey: string): void
}

export function useHostNavigation({
  orderedTabs,
  profiles,
  activeHomeTabId,
  activeSessionTabId,
  onActivateHome,
  onActivateSession,
  onAddHomeTab
}: {
  orderedTabs: OrderedTabEntry[]
  profiles: ConnectionProfile[]
  activeHomeTabId: string | null
  activeSessionTabId: string | null
  onActivateHome(id: string): void
  onActivateSession(id: string): void
  onAddHomeTab(): void
}): HostNavigation {
  const groups = useMemo(() => buildHostGroups(orderedTabs, profiles), [orderedTabs, profiles])
  const activeGroupKey = resolveActiveHostGroupKey(orderedTabs, activeHomeTabId, activeSessionTabId)
  const activeEntryKey = activeHomeTabId
    ? `home:${activeHomeTabId}`
    : activeSessionTabId
      ? `session:${activeSessionTabId}`
      : null
  // Remembers the tab each host was showing so switching back restores it.
  const lastEntryKeyByGroupRef = useRef(new Map<string, string>())

  useEffect(() => {
    if (activeEntryKey) {
      lastEntryKeyByGroupRef.current.set(activeGroupKey, activeEntryKey)
    }
  }, [activeEntryKey, activeGroupKey])

  const onSelectGroup = useCallback(
    (groupKey: string) => {
      const group = groups.find((candidate) => candidate.key === groupKey)
      if (!group) return
      if (!group.entries.length) {
        if (group.kind === 'home') onAddHomeTab()
        return
      }
      const rememberedKey = lastEntryKeyByGroupRef.current.get(groupKey)
      const entry = group.entries.find((candidate) => candidate.key === rememberedKey) ?? group.entries[0]
      if (entry.kind === 'local') {
        onActivateHome(entry.id)
      } else {
        onActivateSession(entry.tab.id)
      }
    },
    [groups, onActivateHome, onActivateSession, onAddHomeTab]
  )

  const activeEntries = groups.find((group) => group.key === activeGroupKey)?.entries ?? []

  return { groups, activeGroupKey, activeEntries, onSelectGroup }
}
