import { useEffect, useState } from 'react'
import type { CreateProfileInput, SshConfigHostSummary } from '@fileterm/core'
import { t } from '../../i18n'
import { DropdownSelect } from '../common/dropdown-select'
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

/** Host input for `authType: 'sshConfig'`: the value is the OpenSSH `Host` alias. */
export function SshConfigHostField({ form, setForm }: { form: CreateProfileInput; setForm: ConnectionFormSetter }) {
  const hosts = useSshConfigHosts()
  const alias = form.host.trim()
  const selected = hosts.find((host) => host.alias.toLowerCase() === alias.toLowerCase())

  const setAlias = (nextAlias: string) =>
    setForm((prev) => ({
      ...prev,
      host: nextAlias,
      // Keep the display name in step with the alias until the user names it.
      name: !prev.name.trim() || prev.name === prev.host ? nextAlias : prev.name
    }))

  return (
    <>
      <label className="span-2">
        {t.sshConfigAlias}:
        <input
          placeholder={t.sshConfigAliasPlaceholder}
          spellCheck={false}
          autoCapitalize="off"
          value={form.host}
          onBlur={(event) => {
            const trimmed = event.target.value.trim()
            if (trimmed !== event.target.value) setAlias(trimmed)
          }}
          onChange={(event) => setAlias(event.target.value)}
        />
      </label>
      {hosts.length > 0 ? (
        <label className="span-2">
          {t.sshConfigPickHost}:
          <DropdownSelect
            value={selected?.alias ?? ''}
            placeholder={t.sshConfigPickHostPlaceholder}
            options={hosts.map((host) => ({ value: host.alias, label: `${host.alias}  (${describeHost(host)})` }))}
            onChange={setAlias}
          />
        </label>
      ) : null}
      <div className="span-2 ssh-field-hint">
        {selected
          ? `${t.sshConfigResolvedTarget} ${describeHost(selected)}${
              selected.proxyJump ? ` ${t.sshConfigResolvedVia} ${selected.proxyJump}` : ''
            }`
          : t.sshConfigAliasHint}
      </div>
    </>
  )
}
