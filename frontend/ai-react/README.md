# AI React Micro-Frontend

React-based AI surfaces embedded into the Vue production shell through
`AiReactEntryShell` and the typed Vue loader in `frontend/vue-app/src/legacy/aiEntryLoader.ts`.

## Active Entries

The active React entries are:

- `ai_monitor` -> `/frontend/ai_monitor.html`
- `llm_eval_lab` -> `/frontend/llm_eval_lab.html`
- `nl_query` -> `/frontend/nl_query.html`
- `dashboard_ai_widget` -> Dashboard embedded widget
- `dispatch_board_ai` -> Dispatch Board embedded drawer

Retired entries are not part of the build:

- `ai_config_center` is owned by Vue `AiConfigCenter.vue`.
- `flight_monitor_ai` is owned by Vue-native Flight Monitor AI panels.
- `flowable_assistant_ai` is owned by Vue-native Flowable AI chat.

## Build Output

Run:

```powershell
npm run build
```

The Vite build writes ignored runtime artifacts to `frontend/static/ai/`, including
`manifest.json`. These generated files are deployment artifacts and are not tracked
by Git. Rebuild this package before serving Vue pages that embed React AI entries.
