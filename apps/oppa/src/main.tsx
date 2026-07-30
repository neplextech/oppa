import React from 'react';
import ReactDOM from 'react-dom/client';
import { HashRouter } from 'react-router-dom';

import App from './App';
import { UpdaterProvider } from './components/updater-provider';

import './index.css';

document.addEventListener('contextmenu', (event) => {
  event.preventDefault();
});

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <UpdaterProvider>
      <HashRouter>
        <App />
      </HashRouter>
    </UpdaterProvider>
  </React.StrictMode>,
);
