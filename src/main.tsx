import React from 'react'
import ReactDOM from 'react-dom/client'
import App from './App'

window.addEventListener('error', (event) => {
  console.error('keym_diag event=window.error', {
    message: event.message,
    filename: event.filename,
    lineno: event.lineno,
    colno: event.colno,
    error: event.error,
  })
})

window.addEventListener('unhandledrejection', (event) => {
  console.error('keym_diag event=unhandledrejection', { reason: event.reason })
})

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
)

console.info('keym_diag event=react_ready')
