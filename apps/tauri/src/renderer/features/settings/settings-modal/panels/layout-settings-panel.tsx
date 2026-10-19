import './layout-settings-panel.css'
import type { FileTermDesktopApi, TabBarLayout } from '@fileterm/core'
import { type LocaleMessages } from '../../../../i18n'
import { RadioCardGroup } from '../../../common/radio-card-group'
import { useSettingsModalContext } from '../context'

type LayoutSettingsPanelContext = {
  t: LocaleMessages
  desktopApi: FileTermDesktopApi | undefined
  tabBarLayout: TabBarLayout
  isSavingTabBarLayout: boolean
  tabBarLayoutError: string | null
  setTabBarLayoutPreference(nextValue: TabBarLayout): void
}

/** A miniature window: tab row on top, then the panes in their real order. */
function TabBarLayoutPreview({ layout }: { layout: TabBarLayout }) {
  const tabCount = layout === 'host-groups' ? 2 : 4

  return (
    <span aria-hidden="true" className={`tab-bar-layout-preview is-${layout}`}>
      <span className="tab-bar-layout-preview-tabs">
        {Array.from({ length: tabCount }, (_, index) => (
          <span key={index} />
        ))}
      </span>
      {layout === 'host-groups' ? (
        <span className="tab-bar-layout-preview-hosts">
          <span />
          <span />
          <span />
        </span>
      ) : null}
      <span className="tab-bar-layout-preview-info" />
      <span className="tab-bar-layout-preview-terminal" />
    </span>
  )
}

export function LayoutSettingsPanel() {
  const { t, desktopApi, tabBarLayout, isSavingTabBarLayout, tabBarLayoutError, setTabBarLayoutPreference } =
    useSettingsModalContext<LayoutSettingsPanelContext>()

  return (
    <div className="settings-panel">
      <section className="settings-section">
        <h3>{t.tabBarLayout}</h3>
        <p className="settings-tools-hint">{t.tabBarLayoutHint}</p>
        <RadioCardGroup
          ariaLabel={t.tabBarLayout}
          className="tab-bar-layout-picker"
          disabled={!desktopApi || isSavingTabBarLayout}
          name="tab-bar-layout"
          onChange={setTabBarLayoutPreference}
          options={[
            {
              value: 'host-groups',
              label: (
                <>
                  <TabBarLayoutPreview layout="host-groups" />
                  <span>{t.tabBarLayoutHostGroups}</span>
                </>
              ),
              description: t.tabBarLayoutHostGroupsHint
            },
            {
              value: 'single-row',
              label: (
                <>
                  <TabBarLayoutPreview layout="single-row" />
                  <span>{t.tabBarLayoutSingleRow}</span>
                </>
              ),
              description: t.tabBarLayoutSingleRowHint
            }
          ]}
          value={tabBarLayout}
        />
        {tabBarLayoutError ? <p className="modal-error">{tabBarLayoutError}</p> : null}
      </section>
    </div>
  )
}
