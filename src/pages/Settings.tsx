import { ThemePicker } from '../components/ThemePicker'
import { MuteComboEditor } from '../components/MuteComboEditor'
import { SoundSettings, DataSettings, AboutSection } from '../components/SettingsPanel'

export function Settings() {
  return (
    <div className="page">
      <div className="card">
        <div className="card-title">设置</div>
        <SoundSettings />
        <ThemePicker />
        <MuteComboEditor />
        <DataSettings />
        <AboutSection />
      </div>
    </div>
  )
}
