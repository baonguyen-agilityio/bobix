// The only bridge between the pages (served from 127.0.0.1) and the app. Pages get these calls, nothing else.
const { contextBridge, ipcRenderer } = require('electron');

const on = ch => cb => ipcRenderer.on(ch, (_e, v) => cb(v));

contextBridge.exposeInMainWorld('bobix', {
  getState: () => ipcRenderer.invoke('state'),
  setSettings: patch => ipcRenderer.invoke('settings', patch),
  resetSettings: () => ipcRenderer.invoke('settings:reset'),
  revealSettings: () => ipcRenderer.invoke('reveal-settings'),
  copyDiagnostics: () => ipcRenderer.invoke('diagnostics'),
  installHooks: () => ipcRenderer.invoke('hooks:install'),
  uninstallHooks: () => ipcRenderer.invoke('hooks:uninstall'),
  testEvent: () => ipcRenderer.invoke('hooks:test'),
  openSession: id => ipcRenderer.invoke('open-session', id),
  answer: (id, choice) => ipcRenderer.invoke('answer', id, choice), // a waiting permission request: allow | always | deny
  setPet: (id, mode) => ipcRenderer.invoke('pet:set', id, mode), // mode: auto | hidden | shown; id '*' = every pet
  onSettings: on('settings'),
  onSelect: on('select'),
  onShowTab: on('show-tab'),
  onOpenFail: on('open-fail'),
  // overlay only
  petClick: id => ipcRenderer.send('pet-click', id),
  setRects: rects => ipcRenderer.send('rects', rects),
  onMouse: on('mouse'),
});
