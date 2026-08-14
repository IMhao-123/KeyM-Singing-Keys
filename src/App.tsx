import { lazy, Suspense } from 'react'
import { HashRouter, Routes, Route, Navigate } from 'react-router-dom'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { Popup } from './components/Popup'
import { Layout } from './components/Layout'
import './styles/global.css'

const Overview = lazy(() => import('./pages/Overview').then((m) => ({ default: m.Overview })))
const Trend = lazy(() => import('./pages/Trend').then((m) => ({ default: m.Trend })))
const Settings = lazy(() => import('./pages/Settings').then((m) => ({ default: m.Settings })))

const isPopup = getCurrentWindow().label === 'popup'

function App() {
  if (isPopup) {
    return <Popup />
  }
  return (
    <HashRouter>
      <Suspense fallback={<div className="state-view"><div className="state-spinner" /></div>}>
        <Routes>
          <Route element={<Layout />}>
            <Route path="/" element={<Overview />} />
            <Route path="/trend" element={<Trend />} />
            <Route path="/settings" element={<Settings />} />
            <Route path="*" element={<Navigate to="/" replace />} />
          </Route>
        </Routes>
      </Suspense>
    </HashRouter>
  )
}

export default App
