// The only bridge between the pages (served from 127.0.0.1) and the app. Pages get these calls, nothing else.
const { contextBridge, ipcRenderer } = require('electron');

const on = ch => cb => ipcRenderer.on(ch, (_e, v) => cb(v));

contextBridge.exposeInMainWorld('bobix', {
  getState: () => ipcRenderer.invoke('state'),
  setSettings: patch => ipcRenderer.invoke('settings', patch),
  resetSettings: () => ipcRenderer.invoke('settings:reset'),
  revealSettings: () => ipcRenderer.invoke('reveal-settings'),
  copyDiagnostics: () => ipcRenderer.invoke('diagnostics'),
  feedbackInfo: () => ipcRenderer.invoke('feedback:info'),
  sendFeedback: v => ipcRenderer.invoke('feedback:send', v), // opens GitHub's form; the person submits it there
  installHooks: () => ipcRenderer.invoke('hooks:install'),
  uninstallHooks: () => ipcRenderer.invoke('hooks:uninstall'),
  testEvent: () => ipcRenderer.invoke('hooks:test'),
  openSession: id => ipcRenderer.invoke('open-session', id),
  answer: (id, choice) => ipcRenderer.invoke('answer', id, choice), // a waiting permission request: allow | always | deny
  petImage: (id, act, opts) => ipcRenderer.invoke('pet:image', id, act, opts), // act: choose | edit | preview | save | cancel | reset
  setPet: (id, mode) => ipcRenderer.invoke('pet:set', id, mode), // mode: auto | hidden | shown; id '*' = every pet
  refreshWork: () => ipcRenderer.invoke('work:refresh'), // GitLab issues, read again now (only while GitLab is on)
  openIssue: url => ipcRenderer.invoke('work:open', url), // in the browser; only links to the configured GitLab
  onSettings: on('settings'),
  onSelect: on('select'),
  onShowTab: on('show-tab'),
  onOpenFail: on('open-fail'),
  // overlay only
  petClick: id => ipcRenderer.send('pet-click', id),
  setRects: rects => ipcRenderer.send('rects', rects),
  onMouse: on('mouse'),
});
