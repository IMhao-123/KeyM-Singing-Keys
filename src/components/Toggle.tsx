interface ToggleProps {
  active: boolean
  onChange: () => void
  label?: string
  disabled?: boolean
}

export function Toggle({ active, onChange, label = '开关', disabled = false }: ToggleProps) {
  return (
    <button
      type="button"
      className={`toggle ${active ? 'active' : ''}`}
      onClick={onChange}
      role="switch"
      aria-label={label}
      aria-checked={active}
      disabled={disabled}
    />
  )
}
