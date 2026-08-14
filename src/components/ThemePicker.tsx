import { getThemeList, setTheme, getTheme } from '../lib/ipc'
import { useAsync, useMutation, useRefreshEvents } from '../hooks/useAsync'
import { StateViews } from './StateViews'
import { ThemeIcon } from './ThemeIcon'

export function ThemePicker() {
  const { data: themes, error, loading, retry } = useAsync(() => getThemeList(), [])
  const { data: current, retry: refetchCurrent } = useAsync(() => getTheme(), [])
  const mutation = useMutation((index: number) => setTheme(index), refetchCurrent)
  useRefreshEvents(['theme-changed'], refetchCurrent)

  return (
    <div className="settings-section">
      <div className="settings-section-title">音效主题</div>
      <StateViews
        loading={loading}
        error={error}
        empty={!themes || themes.length === 0}
        emptyText="暂无主题"
        onRetry={retry}
      >
        <div className="theme-grid">
          {(themes ?? []).map((theme) => (
            <button
              type="button"
              key={theme.index}
              className={`theme-btn ${current === theme.index ? 'active' : ''}`}
              aria-pressed={current === theme.index}
              aria-label={`选择主题 ${theme.name}`}
              disabled={mutation.pending}
              onClick={() => mutation.run(theme.index)}
            >
              <ThemeIcon id={theme.index} />
              <span className="theme-name">{theme.name}</span>
            </button>
          ))}
        </div>
      </StateViews>
      {mutation.error && (
        <div className="settings-message error" role="alert">
          主题保存失败：{mutation.error}
        </div>
      )}
    </div>
  )
}
