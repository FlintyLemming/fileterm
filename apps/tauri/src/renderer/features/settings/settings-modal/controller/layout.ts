import { useState } from 'react'
import type { TabBarLayout } from '@fileterm/core'
import { t } from '../../../../i18n'
import { useTabBarLayout } from '../../../layout/use-tab-bar-layout'
import type { SettingsModalState } from './state'

export function useLayoutSettingsController({ state }: { state: SettingsModalState }) {
  const { desktopApi } = state
  const [tabBarLayout, setTabBarLayout] = useTabBarLayout(desktopApi)
  const [isSavingTabBarLayout, setIsSavingTabBarLayout] = useState(false)
  const [tabBarLayoutError, setTabBarLayoutError] = useState<string | null>(null)

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

  return {
    tabBarLayout,
    isSavingTabBarLayout,
    tabBarLayoutError,
    setTabBarLayoutPreference
  }
}
