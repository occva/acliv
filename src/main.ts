
import { mount } from 'svelte'
import 'highlight.js/styles/github-dark.min.css'
import App from './App.svelte'

const root = document.documentElement
const runtimeWindow = window as unknown as Record<string, unknown>
const isDesktopRuntime = Boolean(
    runtimeWindow.__TAURI_INTERNALS__ || runtimeWindow.__TAURI__ || runtimeWindow.__TAURI_IPC__,
)

root.dataset.runtime = isDesktopRuntime ? 'desktop' : 'web'
if (navigator.platform.toLowerCase().includes('mac')) {
    root.dataset.platform = 'macos'
}

const app = mount(App, {
    target: document.getElementById('app')!,
})

export default app
