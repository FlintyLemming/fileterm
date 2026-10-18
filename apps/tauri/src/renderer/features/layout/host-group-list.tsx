import './host-group-list.css'
import { useRef } from 'react'
import { tabStatusClass } from '../../app/app-utils'
import { t } from '../../i18n'
import { StatusIndicator } from '../common/status-indicator'
import { VerticalScrollbar } from '../common/vertical-scrollbar'
import type { HostGroup } from './host-groups'
import type { HostNavigation } from './use-host-navigation'

function hostGroupIcon(group: HostGroup) {
  if (group.kind === 'home') return 'home'
  if (group.kind === 'local') return 'terminal'
  return group.profileType === 'serial' ? 'usb' : 'dns'
}

export function hostGroupLabel(group: HostGroup) {
  if (group.kind === 'home') return t.hostRailHome
  if (group.kind === 'local') return t.localTerminal
  return group.title || t.untitledTab
}

/** Machine list: picking a host switches the tab bar to that host's tabs. */
export function HostGroupList({
  navigation,
  includeHome = true,
  collapsed = false
}: {
  navigation: HostNavigation
  includeHome?: boolean
  collapsed?: boolean
}) {
  const scrollRef = useRef<HTMLUListElement>(null)
  const groups = includeHome ? navigation.groups : navigation.groups.filter((group) => group.kind !== 'home')

  return (
    <div className={`host-group-list ${collapsed ? 'is-collapsed' : ''}`}>
      <ul aria-label={t.hostRailTitle} className="host-group-list-viewport" ref={scrollRef}>
        {groups.map((group) => {
          const label = hostGroupLabel(group)
          const isActive = group.key === navigation.activeGroupKey
          const tabCount = group.entries.length
          return (
            <li key={group.key}>
              <button
                aria-current={isActive ? 'true' : undefined}
                className={`host-group-item ${isActive ? 'is-active' : ''}`}
                title={group.subtitle ? `${label}\n${group.subtitle}` : label}
                type="button"
                onClick={() => navigation.onSelectGroup(group.key)}
              >
                <span aria-hidden="true" className="host-group-icon material-symbols-outlined">
                  {hostGroupIcon(group)}
                </span>
                {!collapsed ? (
                  <span className="host-group-text">
                    <strong>{label}</strong>
                    {group.subtitle ? <small>{group.subtitle}</small> : null}
                  </span>
                ) : null}
                {!collapsed && group.status ? (
                  <StatusIndicator aria-hidden="true" status={tabStatusClass(group.status)} />
                ) : null}
                {!collapsed && tabCount > 1 ? (
                  <span aria-label={`${tabCount} ${t.hostRailTabCount}`} className="host-group-count">
                    {tabCount}
                  </span>
                ) : null}
              </button>
            </li>
          )
        })}
      </ul>
      <VerticalScrollbar ariaLabel={t.hostRailTitle} scrollRef={scrollRef} />
    </div>
  )
}
