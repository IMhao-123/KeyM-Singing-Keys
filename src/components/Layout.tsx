import { NavLink, Outlet } from 'react-router-dom'
import { RuntimeHealthBanner } from './RuntimeHealthBanner'

export function Layout() {
  return (
    <div className="layout">
      <header className="topbar">
        <div className="topbar-logo">键标</div>
        <nav className="topbar-nav">
          <NavLink to="/" end className={({ isActive }) => (isActive ? 'nav-link active' : 'nav-link')}>
            概览
          </NavLink>
          <NavLink to="/trend" className={({ isActive }) => (isActive ? 'nav-link active' : 'nav-link')}>
            趋势
          </NavLink>
          <NavLink
            to="/settings"
            className={({ isActive }) => (isActive ? 'nav-link active' : 'nav-link')}
          >
            设置
          </NavLink>
        </nav>
      </header>
      <RuntimeHealthBanner />
      <main className="page-container">
        <Outlet />
      </main>
    </div>
  )
}
