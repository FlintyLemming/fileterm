import './ssh-config-host-field.css'
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import type { CreateProfileInput, SshConfigHostSummary } from '@fileterm/core'
import { t } from '../../i18n'
import type { ConnectionFormSetter } from './connection-modal-utils'

function useSshConfigHosts() {
  const desktopApi = window.fileterm
  const [hosts, setHosts] = useState<SshConfigHostSummary[]>([])

  useEffect(() => {
    if (!desktopApi) return
    let canceled = false
    desktopApi
      .listSshConfigHosts()
      .then((nextHosts) => {
        if (!canceled) setHosts(nextHosts)
      })
      // A missing or unreadable ~/.ssh/config only removes the suggestions.
      .catch(() => undefined)
    return () => {
      canceled = true
    }
  }, [desktopApi])

  return hosts
}

function describeHost(host: SshConfigHostSummary) {
  return `${host.user}@${host.hostname}:${host.port}`
}

/** Keep hosts whose alias or resolved address contains the typed text. */
export function filterSshConfigHosts(hosts: SshConfigHostSummary[], query: string) {
  const needle = query.trim().toLowerCase()
  if (!needle) return hosts
  return hosts.filter(
    (host) => host.alias.toLowerCase().includes(needle) || host.hostname.toLowerCase().includes(needle)
  )
}

/**
 * Host combobox for `authType: 'sshConfig'`. The value is the OpenSSH `Host`
 * alias: type freely or pick one of the aliases read from ~/.ssh/config; the
 * list filters live while typing.
 */
export function SshConfigHostField({ form, setForm }: { form: CreateProfileInput; setForm: ConnectionFormSetter }) {
  const hosts = useSshConfigHosts()
  const inputRef = useRef<HTMLInputElement>(null)
  const menuRef = useRef<HTMLDivElement>(null)
  const [open, setOpen] = useState(false)
  const [highlighted, setHighlighted] = useState(0)
  const alias = form.host.trim()
  const selected = hosts.find((host) => host.alias.toLowerCase() === alias.toLowerCase())
  const matches = useMemo(() => filterSshConfigHosts(hosts, form.host), [hosts, form.host])
  const menuOpen = open && matches.length > 0

  const setAlias = (nextAlias: string) =>
    setForm((prev) => ({
      ...prev,
      host: nextAlias,
      // Keep the display name in step with the alias until the user names it.
      name: !prev.name.trim() || prev.name === prev.host ? nextAlias : prev.name
    }))

  const choose = (host: SshConfigHostSummary) => {
    setAlias(host.alias)
    setOpen(false)
  }

  useEffect(() => {
    setHighlighted(0)
  }, [form.host])

  useEffect(() => {
    if (!menuOpen) return
    const handlePointerDown = (event: PointerEvent) => {
      const target = event.target
      if (!(target instanceof Node)) return
      if (inputRef.current?.contains(target) || menuRef.current?.contains(target)) return
      setOpen(false)
    }
    window.addEventListener('pointerdown', handlePointerDown, true)
    return () => window.removeEventListener('pointerdown', handlePointerDown, true)
  }, [menuOpen])

  useLayoutEffect(() => {
    const input = inputRef.current
    const menu = menuRef.current
    if (!menuOpen || !input || !menu) return
    const rect = input.getBoundingClientRect()
    const viewportMargin = 8
    menu.style.left = `${rect.left}px`
    menu.style.width = `${rect.width}px`
    const menuHeight = menu.getBoundingClientRect().height
    const below = rect.bottom + 4
    menu.style.top =
      below + menuHeight > window.innerHeight - viewportMargin && rect.top - menuHeight - 4 > viewportMargin
        ? `${rect.top - menuHeight - 4}px`
        : `${below}px`
  }, [menuOpen, matches])

  useEffect(() => {
    if (!menuOpen) return
    menuRef.current?.querySelector<HTMLElement>(`[data-index="${highlighted}"]`)?.scrollIntoView({ block: 'nearest' })
  }, [menuOpen, highlighted])

  return (
    <>
      <label className="span-2">
        {t.sshConfigAlias}:
        <input
          ref={inputRef}
          role="combobox"
          aria-expanded={menuOpen}
          aria-autocomplete="list"
          autoComplete="off"
          autoCapitalize="off"
          spellCheck={false}
          placeholder={hosts.length ? t.sshConfigAliasPickPlaceholder : t.sshConfigAliasPlaceholder}
          value={form.host}
          onFocus={() => setOpen(true)}
          onBlur={(event) => {
            const trimmed = event.target.value.trim()
            if (trimmed !== event.target.value) setAlias(trimmed)
          }}
          onChange={(event) => {
            setAlias(event.target.value)
            setOpen(true)
          }}
          onKeyDown={(event) => {
            if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
              event.preventDefault()
              if (!menuOpen) {
                setOpen(true)
                return
              }
              const step = event.key === 'ArrowDown' ? 1 : -1
              setHighlighted((current) => (current + step + matches.length) % matches.length)
            } else if (event.key === 'Enter' && menuOpen) {
              event.preventDefault()
              const host = matches[highlighted]
              if (host) choose(host)
            } else if (event.key === 'Escape' && menuOpen) {
              // Close only the list, not the surrounding connection dialog.
              event.preventDefault()
              event.stopPropagation()
              setOpen(false)
            }
          }}
        />
      </label>
      <div className="span-2 ssh-field-hint">
        {selected
          ? `${t.sshConfigResolvedTarget} ${describeHost(selected)}${
              selected.proxyJump ? ` ${t.sshConfigResolvedVia} ${selected.proxyJump}` : ''
            }`
          : t.sshConfigAliasHint}
      </div>
      {menuOpen && typeof document !== 'undefined'
        ? createPortal(
            <div
              ref={menuRef}
              className="context-menu dropdown-select-menu ssh-config-host-menu"
              role="listbox"
              style={{ position: 'fixed' }}
            >
              {matches.map((host, index) => (
                <button
                  key={host.alias}
                  data-index={index}
                  className={`ssh-config-host-option${index === highlighted ? ' is-highlighted' : ''}${
                    host === selected ? ' is-selected' : ''
                  }`}
                  role="option"
                  aria-selected={host === selected}
                  tabIndex={-1}
                  type="button"
                  // Keep focus in the input so typing can continue.
                  onMouseDown={(event) => event.preventDefault()}
                  onMouseEnter={() => setHighlighted(index)}
                  onClick={() => choose(host)}
                >
                  <span className="ssh-config-host-option__alias">{host.alias}</span>
                  <span className="ssh-config-host-option__target">{describeHost(host)}</span>
                </button>
              ))}
            </div>,
            document.body
          )
        : null}
    </>
  )
}
