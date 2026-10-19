import { useEffect, useState } from 'react'
import { DEFAULT_COMPACT_TAB_BAR, type FileTermDesktopApi } from '@fileterm/core'

function normalizeCompactTabBar(value: unknown): boolean {
  return typeof value === 'boolean' ? value : DEFAULT_COMPACT_TAB_BAR
}

/**
 * Whether the top tab bar uses the compact height, kept current from
 * preference events. Only the settings page writes it; the setter is for its
 * optimistic updates.
 */
export function useCompactTabBar(desktopApi: FileTermDesktopApi | undefined, initialValue?: boolean) {
  const [compactTabBar, setCompactTabBar] = useState(() => normalizeCompactTabBar(initialValue))
  const hasInitialValue = initialValue !== undefined

  useEffect(() => {
    if (!desktopApi) {
      return
    }

    let canceled = false
    // Startup normally hands over the saved value; read it when that failed.
    if (!hasInitialValue) {
      void desktopApi
        .getUiPreferences()
        .then((preferences) => {
          if (!canceled) setCompactTabBar(normalizeCompactTabBar(preferences.compactTabBar))
        })
        .catch(() => {
          // Keep the default height when the preference cannot be read.
        })
    }
    const unsubscribe = desktopApi.onUiPreferencesChanged((preferences) => {
      if (!canceled) setCompactTabBar(normalizeCompactTabBar(preferences.compactTabBar))
    })

    return () => {
      canceled = true
      unsubscribe()
    }
  }, [desktopApi, hasInitialValue])

  return [compactTabBar, setCompactTabBar] as const
}
