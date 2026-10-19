import './layout-settings-section.css'
import type { FileTermDesktopApi, TabBarLayout } from '@fileterm/core'
import { type LocaleMessages } from '../../../../i18n'
import { RadioCardGroup } from '../../../common/radio-card-group'
import { SelectionControl } from '../../../common/selection-control'
import { useSettingsModalContext } from '../context'

type LayoutSettingsSectionContext = {
  t: LocaleMessages
  desktopApi: FileTermDesktopApi | undefined
  tabBarLayout: TabBarLayout
  isSavingTabBarLayout: boolean
  tabBarLayoutError: string | null
  setTabBarLayoutPreference(nextValue: TabBarLayout): void
  compactTabBar: boolean
  isSavingCompactTabBar: boolean
  compactTabBarError: string | null
  setCompactTabBarPreference(nextValue: boolean): void
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

export function LayoutSettingsSection() {
  const {
    t,
    desktopApi,
    tabBarLayout,
    isSavingTabBarLayout,
    tabBarLayoutError,
    setTabBarLayoutPreference,
    compactTabBar,
    isSavingCompactTabBar,
    compactTabBarError,
    setCompactTabBarPreference
  } = useSettingsModalContext<LayoutSettingsSectionContext>()

  return (
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
      <div className="overview-preference-list">
        <label aria-busy={isSavingCompactTabBar} className="overview-preference-row" inert={isSavingCompactTabBar}>
          <span className="overview-preference-copy">
            <strong>{t.compactTabBar}</strong>
            <p>{t.compactTabBarHint}</p>
          </span>
          <span className="command-toggle overview-preference-toggle">
            <SelectionControl
              checked={compactTabBar}
              disabled={!desktopApi}
              onChange={(event) => setCompactTabBarPreference(event.target.checked)}
              type="checkbox"
            />
          </span>
        </label>
      </div>
      {compactTabBarError ? <p className="modal-error">{compactTabBarError}</p> : null}
    </section>
  )
}
