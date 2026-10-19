import { useState } from 'react'
import type { TabBarLayout } from '@fileterm/core'
import { t } from '../../../../i18n'
import { useCompactTabBar } from '../../../layout/use-compact-tab-bar'
import { useTabBarLayout } from '../../../layout/use-tab-bar-layout'
import type { SettingsModalState } from './state'

export function useLayoutSettingsController({ state }: { state: SettingsModalState }) {
  const { desktopApi } = state
  const [tabBarLayout, setTabBarLayout] = useTabBarLayout(desktopApi)
  const [isSavingTabBarLayout, setIsSavingTabBarLayout] = useState(false)
  const [tabBarLayoutError, setTabBarLayoutError] = useState<string | null>(null)
  const [compactTabBar, setCompactTabBar] = useCompactTabBar(desktopApi)
  const [isSavingCompactTabBar, setIsSavingCompactTabBar] = useState(false)
  const [compactTabBarError, setCompactTabBarError] = useState<string | null>(null)

  const setTabBarLayoutPreference = (nextValue: TabBarLayout) => {
    if (!desktopApi || isSavingTabBarLayout || nextValue === tabBarLayout) {
      return
    }

    const previousValue = tabBarLayout
    setTabBarLayout(nextValue)
    setTabBarLayoutError(null)
    setIsSavingTabBarLayout(true)
    void desktopApi
      .setUiPreferences({ tabBarLayout: nextValue })
      .then((preferences) => setTabBarLayout(preferences.tabBarLayout))
      .catch(() => {
        setTabBarLayout(previousValue)
        setTabBarLayoutError(t.tabBarLayoutSaveFailed)
      })
      .finally(() => setIsSavingTabBarLayout(false))
  }

  const setCompactTabBarPreference = (nextValue: boolean) => {
    if (!desktopApi || isSavingCompactTabBar || nextValue === compactTabBar) {
      return
    }

    const previousValue = compactTabBar
    setCompactTabBar(nextValue)
    setCompactTabBarError(null)
    setIsSavingCompactTabBar(true)
    void desktopApi
      .setUiPreferences({ compactTabBar: nextValue })
      .then((preferences) => setCompactTabBar(preferences.compactTabBar))
      .catch(() => {
        setCompactTabBar(previousValue)
        setCompactTabBarError(t.compactTabBarSaveFailed)
      })
      .finally(() => setIsSavingCompactTabBar(false))
  }

  return {
    tabBarLayout,
    isSavingTabBarLayout,
    tabBarLayoutError,
    setTabBarLayoutPreference,
    compactTabBar,
    isSavingCompactTabBar,
    compactTabBarError,
    setCompactTabBarPreference
  }
}
