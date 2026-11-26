import React from 'react';
import ReactDOM from 'react-dom/client';
import { HashRouter } from 'react-router-dom';

import App from './App';
import { UpdaterProvider } from './components/updater-provider';
import { detectMobileOs } from './lib/platform';
import { isWindowsTauri } from './lib/window-platform';

import './index.css';
import './mobile.css';

// Decide the layout family before the first paint; capabilities reported by
// the agent still decide which features are shown.
const mobileOs = detectMobileOs();
document.documentElement.dataset.runtime = mobileOs ? 'mobile' : 'desktop';
if (mobileOs) document.documentElement.dataset.os = mobileOs;

document.addEventListener('contextmenu', (event) => {
  event.preventDefault();
});

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    {!mobileOs && isWindowsTauri() && <div data-tauri-frame-tb role="group" aria-label="Window controls" />}
    <UpdaterProvider>
      <HashRouter>
        <App />
      </HashRouter>
    </UpdaterProvider>
  </React.StrictMode>,
);
