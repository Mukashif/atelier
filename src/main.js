import '@fontsource-variable/inter';
import '@fontsource-variable/archivo/wdth.css';
import '@fontsource/jetbrains-mono/400.css';
import './app.css';
import { mount } from 'svelte';

// Outside Tauri (plain browser during development) use sample data.
if (import.meta.env.DEV && !('__TAURI_INTERNALS__' in window)) {
  await import('./dev-mock.js');
}
const { default: App } = await import('./App.svelte');

export default mount(App, { target: document.getElementById('app') });
