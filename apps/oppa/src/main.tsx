import React from 'react';
import ReactDOM from 'react-dom/client';
import { HashRouter } from 'react-router-dom';

import App from './App';
import { UpdaterProvider } from './components/updater-provider';
import { isWindowsTauri } from './lib/window-platform';

import './index.css';

document.addEventListener('contextmenu', (event) => {
  event.preventDefault();
});

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    {isWindowsTauri() && <div data-tauri-frame-tb role="group" aria-label="Window controls" />}
    <UpdaterProvider>
      <HashRouter>
        <App />
      </HashRouter>
    </UpdaterProvider>
  </React.StrictMode>,
);
