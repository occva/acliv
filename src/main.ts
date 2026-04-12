
import { mount } from 'svelte'
import 'highlight.js/styles/github-dark.min.css'
import App from './App.svelte'

const app = mount(App, {
    target: document.getElementById('app')!,
})

export default app
