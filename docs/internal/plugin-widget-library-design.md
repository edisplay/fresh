# Plugin widget library — design proposal

Status: proposal
Author: staff-eng review, branch `claude/plugin-ui-component-library-wO76I`
Scope: shared UI components for the Fresh plugin runtime
Related: `docs/internal/UNIFIED_UI_FRAMEWORK_PLAN.md`,
`docs/internal/unified-hit-test-theme-plan.md`,
`docs/internal/unified-keybinding-resolution.md`,
`docs/internal/event-dispatch-architecture.md`,
`docs/internal/visual-layout-unification.md`,
`docs/internal/plugin-usability-review.md`,
`docs/internal/settings-controls-usability-report.md`

> Note. The brief referenced `lib/text_area.ts` and
> `docs/internal/search-replace-ux-improvements.md`; neither exists in
> the tree. Where the brief asked for them as evidence, we substitute
> the equivalent live sources: the Rust `view/controls/*` modules,
> per-plugin call-sites (`search_replace.ts`, `git_log.ts`,
> `dashboard.ts`, `audit_mode.ts`, `lib/finder.ts`), and the open
> §-items in `plugin-usability-review.md` and
> `settings-controls-usability-report.md`.

---

## 1. Recommendation

**Hybrid: a Rust-resident widget runtime with a thin TypeScript
declarative front-end. Plugins describe widgets as data, the host
reconciles, owns layout/hit-test/focus, and emits semantic events.
The existing `setVirtualBufferContent` primitive stays as the
escape-hatch.**

This is the only shape that simultaneously satisfies the five
constraints in the brief:

| Constraint | Pure-TS on `setVirtualBufferContent` | Pure Rust-core | Hybrid (proposed) |
|---|---|---|---|
| Per-keystroke cost | Full re-serialization + buffer replace per char (today: `delete-all + insert-all + bulk overlay add`, `crates/fresh-editor/src/app/virtual_buffers.rs:356`) | One IPC call carries `set_input_value`, no JS render | TS sends a delta against last spec; reconciler in Rust applies the minimum mutation |
| Theme | Plugin must hard-code `"syntax.keyword"` etc.; no abstraction over palette intent | Widget asks `theme.button_fg()` directly | Widgets carry *roles* (`Role::Action`, `Role::Toggle`); core resolves to the active `Theme` (`crates/fresh-editor/src/view/theme/types.rs:1116`) |
| Reach (built-ins) | Built-ins stay in `view/controls/*`; no sharing | Plugins call the same widgets the settings pane uses | Settings keeps using the Rust controls; plugins describe widgets with the same `Spec` enum, rendered by the same Rust code |
| Backwards compat | Native; nothing to do | Hard cut-over; hundreds of `TextPropertyEntry[]` call-sites | New API is additive; `setVirtualBufferContent` and `defineMode` stay; widgets are mounted *into* a virtual buffer the plugin already owns |
| Sandboxing | No new capabilities granted (good) | Risk: a Rust widget that "does the thing" gives the plugin capabilities it didn't have | Widgets emit *events*, never side-effects. A `Button` fires `onActivate` back into the plugin; the plugin still calls existing APIs to do the work. No new capability bits. |

### Why not pure TypeScript on the existing primitive

It is tempting because nothing on the Rust side changes and 100% of
the existing call-sites keep working. But:

- Per-keystroke cost is non-trivial. `set_virtual_buffer_content` is
  a *full* replacement — every keystroke deserializes the entries
  array, deletes the buffer's bytes, inserts the new text, and
  rebuilds the inline-overlay tree
  (`virtual_buffers.rs:356–405`).
- Theme integrity is unenforceable. Plugins already each ship their own
  palette guesses; a TS-only library has no leverage to make
  high-contrast or color-blind themes work.
- Built-in surfaces (`crates/fresh-editor/src/view/settings/`,
  `view/controls/{button,dropdown,toggle,number_input,text_input,text_list}`)
  cannot share code with TS. The split codebase is exactly what
  `UNIFIED_UI_FRAMEWORK_PLAN.md` set out to fix; doubling it would be a
  regression.
- Hit-testing math (`buffer_row → which widget`) keeps living in every
  plugin. `dashboard.ts` already has a `rowActions: Map<row, Range[]>`,
  `audit_mode.ts` has `entryPropsByRow`, `git_log.ts` does a
  `binarySearch(logRowByteOffsets)`. The library cannot remove this
  without owning hit-test, and the *information needed for hit-test*
  (visual columns, wide chars, ANSI escapes) lives Rust-side
  (`docs/internal/visual-layout-unification.md`).

### Why not pure Rust-core

- Capability inflation is the killer. A Rust `FilePicker` that reads
  the disk gives every plugin the disk-read capability via the widget
  call. The current model — plugins call `editor.openFile` and the host
  enforces — must be preserved.
- The flag-day rewrite is a non-starter. There are 107 plugins on
  `main`. Migration must be opt-in plugin-by-plugin.
- We lose the imperative-buffer escape hatch that
  `audit_mode.ts`, `code-tour.ts`, and the LSP plugins all rely on.

### Why hybrid wins

The Rust side already has the right shape: each control in
`view/controls/*` is `<Name>State + <Name>Layout + <Name>Colors +
render_<name>()`. `UNIFIED_UI_FRAMEWORK_PLAN.md` Steps 1–6 already
extracted `point_in_rect()` and `FocusManager<T>` and migrated Settings
onto them. Steps 7–8 (Menu/Tabs) and the unwritten "TS plugin mirrors"
(`controls.ts`, `vbuffer.ts`) are the missing bridge. This proposal is
that bridge.

The TS side gets a **declarative widget tree** (VS Code TreeView shape),
the Rust side runs a **layered Component compositor** (Helix shape) with
**transient-style keymaps** (Magit shape), and the imperative
`setVirtualBufferContent` survives unchanged for the rare cases where
declarative is the wrong fit (Neovim/modern-Emacs shape). Webview-style
HTML escape hatches are explicitly rejected (Sublime's `on_navigate`
href-dispatch is the safe analogue, and we already have it via
`editor.on("mouse_click", …)`).

---

## 2. Widget catalogue

Distilled from `search_replace.ts`, `git_log.ts`, `dashboard.ts`,
`audit_mode.ts`, `lib/finder.ts`, and the Rust `view/controls/*`
modules. "Today" = needed by an existing call-site; "next" = needed
to close a §-item in `plugin-usability-review.md` or
`settings-controls-usability-report.md`; "later" = speculative.

| Widget | Used by | Props | Internal state | Events | Cohort |
|---|---|---|---|---|---|
| `TextInput` (single line) | `search_replace.ts` (search/replace fields), Settings string controls | `value`, `placeholder`, `password?`, `validator?` | cursor byte offset, selection, undo ring, IME composition | `onChange(value)`, `onSubmit`, `onCursorMove` | today |
| `TextArea` (multi-line) | `search_replace.ts` (the implied lib/text_area.ts), composer plugins | `value`, `lineWrap`, `tabWidth` | as above + scroll offset | `onChange`, `onSubmit(modKey)` | today |
| `Toggle` / `Checkbox` | `search_replace.ts` (case/word/regex), Settings | `checked`, `label` | — | `onToggle(next)` | today |
| `Button` | `git_log.ts` toolbar, `dashboard.ts`, Settings | `label`, `kind: primary\|danger\|ghost`, `disabled` | hover, pressed | `onActivate` | today |
| `List` (virtual-scrolled, item-keyed) | `lib/finder.ts`, `git_log.ts` | `items: Array<{key, render, data?}>`, `selectedKey` | scroll offset, hover index | `onSelect(key)`, `onActivate(key)`, `onContext(key)` | today |
| `Tree` (expand/collapse, lazy children) | `search_replace.ts` (file → matches), `audit_mode.ts` (files → hunks), file-explorer | `roots`, `expandedKeys: Set<key>`, `selectedKey`, `provider: getChildren(key)` | scroll, hover | `onToggleExpand(key)`, `onSelect(key)`, `onActivate(key)`, `onContext(key)` | today |
| `Panel` | every panelled plugin | `title`, `toolbar?: Toolbar`, `body: Widget`, `footer?: HintBar` | focus index across children | `onClose` | today |
| `Toolbar` | `git_log.ts`, `audit_mode.ts` | `items: Array<Button \| Separator \| ToggleGroup>` | — | per-item events | today |
| `HintBar` | every plugin's "?" footer | `entries: Array<{keys, label}>` | — | — | today |
| `Tabs` / `Group` | `git_log.ts` buffer group, Settings categories | `tabs: Array<{key, title, badge?}>`, `activeKey` | — | `onSelect(key)` | today |
| `Prompt` (modal input) | `lib/finder.ts`, every confirm | `title`, `body: Widget`, `actions: Button[]` | as Panel | `onAction(key)` | today |
| `Transient` (key-grouped command menu) | currently absent; needed by `git_log`, `search_replace` | `groups: [{title, entries: [{keys, label, command}]}]` | `chord` state | `onCommand(id)` | next |
| `Table` (columns, sortable, selectable rows) | `git_log.ts` log, `find_references.ts`, audit | `columns`, `rows`, `sortKey?`, `selectedRowKey?` | scroll, hover, sort | `onSort`, `onSelectRow`, `onActivateRow` | next |
| `KeybindingList` | mirror Rust `keybinding_list/` | as Settings | as Settings | `onChange(binding[])` | next |
| `MapInput` | mirror Rust `map_input/` | as Settings | as Settings | `onChange(map)` | next |
| `Diagnostic` / `InlineHint` | LSP plugins | `severity`, `message`, `source?` | — | — | next |
| `ProgressBar`, `Spinner` | indexer plugins | `progress?: 0..1`, `label?` | — | — | later |
| `Dropdown` (closed-set picker) | Settings | `options`, `selectedKey` | open?, hover | `onSelect` | later |

A `Button` and a `Toggle` carry a *role* (`Role::Action`,
`Role::Destructive`, `Role::Selected`, `Role::Disabled`). The library
maps roles → theme keys; there is no `fg: [r,g,b]` in widget props.
Plugins that genuinely need a custom color use the imperative escape
hatch.

The catalogue is intentionally short. Anything not on this list (rich
text, syntax-highlighted code, custom drawing) lives inside an
imperative-virtual-buffer widget — a `RawBuffer` widget whose body is
just a `TextPropertyEntry[]` produced by the plugin.

---

## 3. Layout primitive

**Line-oriented flex along the row axis, absolute along the column
axis, with a small Rect-based composition layer.** Three reasons:

1. The terminal is row-major. Every plugin already thinks in rows. A
   web-style flex-everywhere model would force plugins to reason about
   things terminals don't have (sub-row positioning).
2. The *interesting* layout question is column distribution: a
   `Toolbar` packs left-to-right, a `Panel`'s body fills, a `HintBar`
   packs right-to-left. That's `flex-row` with `grow/shrink` on
   children, the same shape the Rust controls already have implicitly.
3. The terminal-line-wrap problem (toolbars must not wrap) is solved by
   marking widgets `wrap: "never"` and letting the host *clip* with
   ellipsis — never line-wrap a widget, never let line-wrap split a
   widget across rows.

API shape (TS):

```
type Spec =
  | { kind: "row"; children: Spec[]; wrap?: "never" | "soft" }
  | { kind: "col"; children: Spec[] }
  | { kind: "fill"; child: Spec }
  | { kind: "fixed"; rows: number; child: Spec }
  | { kind: "widget"; type: WidgetType; props: ...; key: string }
  | { kind: "raw"; entries: TextPropertyEntry[] };
```

`raw` is the integration with `setVirtualBufferContent` —
existing plugins gain widgets *inside* a buffer they already own
without rewriting their renderer. The host composes `raw` regions with
widget regions on the row axis.

Horizontal scroll is a property of `RawBuffer`, `Table`, and `List`
content; it is not a layout-level concern. Toolbars set
`wrap: "never"` and clip; this is consistent with how the Rust
`view/controls/keybinding_list/` already truncates.

---

## 4. Focus / keyboard model

A **panel-level focus stack** with one *Tab cycle* per panel, computed
from the widget tree's flattened tab-stops in declaration order. Each
panel has a single active widget; the host paints focus styling. This
replaces the per-plugin `focusedField` enums in `search_replace.ts`.

### Interaction with `defineMode`

Today plugins call `editor.defineMode("search_replace", [["Tab", "search_replace_tab"], …])`.
That model is fine for *panel-level* commands but wrong for
*widget-level* keys (Backspace, Arrow keys inside a `TextInput`). The
proposal:

- Each widget has a built-in keymap the host handles **before** the
  plugin's mode bindings see the key. A `TextInput` consumes
  Backspace/Arrow/Home/End; a `Tree` consumes Left/Right/Space.
- The plugin's mode bindings remain authoritative for **panel-level**
  keys (Tab, Enter when nothing claims it, Escape, plugin-defined
  chords like `g g`).
- This is exactly Helix's bubble-up `EventResult::Consumed | Ignored`
  semantics, translated across IPC: the host runs widget keymaps
  synchronously; only on `Ignored` does it dispatch the plugin's
  defined-mode handler. No round-trip on keystrokes that widgets
  already eat.

This composes cleanly with `unified-keybinding-resolution.md` (single
resolution path through `KeybindingResolver`): widget keymaps are an
extra layer *above* the resolver, registered when a widget mounts and
unregistered on unmount.

### Global pass-through

Global shortcuts (e.g. `C-,` settings, `C-q` quit) live above
everything. The order is:

1. Global resolver
2. Active widget's built-in keymap
3. Active panel's `defineMode` bindings
4. Buffer/normal-mode bindings (only if `allowTextInput` and unfocused)

A widget can opt out of step 1 only inside a `Prompt` (modal). This
covers the §18 "global-shortcut pass-through" question that the brief
asked about: it lives at the dispatcher, not per-widget.

### Chord bindings

Chords (`g g`, `Space f`) keep working through the existing
`KeybindingResolver` chord state. The widget layer is stateless w.r.t.
chords — a half-finished chord (`g`) is held by the resolver, not by
the widget.

### Terminal constraint

Shift+Enter ≡ Enter at the terminal, Shift+Alt+Enter ≡ Alt+Enter. We
do not bind Shift+Enter as a distinct key. `TextArea` submit is
**Enter** if `singleSubmitsOnEnter`, else **Alt+Enter** (or **Ctrl+J**
where preferred), and Enter inserts a newline. The widget exposes
`submit: "enter" | "altEnter"` and the plugin picks; the library's
default for multi-line inputs is `altEnter`. The `HintBar` shows the
chosen key string.

---

## 5. Mouse model

The host owns all hit-testing. The plugin never sees `(buffer_row,
buffer_col)`; it receives semantic events.

- Each widget instance has a `Rect` (rows × cols) computed by the
  layout. Hit-test dispatcher lives in
  `app/event_dispatch.rs` (per `event-dispatch-architecture.md` Phase
  2) and answers `(col, row) → WidgetHandle` via z-ordered
  `CachedLayout::region_at` (per `unified-hit-test-theme-plan.md`).
- The widget runtime translates a hit into a widget-local event:
  - `Tree` → `onToggleExpand(key)` if column intersects the disclosure
    glyph, else `onSelect(key)`; double-click → `onActivate(key)`.
  - `List` → `onSelect/onActivate(key)`.
  - `Button` → `onActivate`.
  - `TextInput` → cursor placement, selection drag.
- Drag and hover get first-class events:
  `onPress`, `onDrag(dx, dy)`, `onRelease`, `onHover(true|false)`. The
  host coalesces hover into one event per row change.
- Scroll: wheel events route to the deepest widget that declares
  `scrollable: true`. The host owns the scroll offset; widgets that
  need to know it (virtualized `List`, `Tree`) get it via
  `scrollOffset` callback.

This eliminates `dashboard.ts:rowActions`, `audit_mode.ts:entryPropsByRow`,
`git_log.ts:logRowByteOffsets` binary search, and the hundreds of
`buffer_row` arithmetic call-sites the brief mentioned.

---

## 6. State model

**Reactive on the Rust side, declarative on the TS side.** The plugin
re-emits a `Spec` whenever its model changes; the host runs a keyed
reconciler against the previous `Spec` for that panel and applies a
minimal patch.

This is structurally the React-virtual-DOM model, intentionally. It
matters because:

- The plugin author keeps writing the imperative code they already
  write (a `redraw()` function that builds a tree from current
  state). That style is what `search_replace.ts:render` and
  `dashboard.ts:emit` already do; we are only changing what they emit.
- Widget *internal* state (cursor position, scroll offset, expanded
  keys, pressed-but-not-yet-released) is owned by the **Rust** widget
  instance, keyed by stable `key`. The plugin never sees it. This is
  what makes per-keystroke editing free of round-trips: a keystroke in
  a `TextInput` mutates Rust-side state and emits `onChange(value)`
  back to the plugin once; if the plugin doesn't change the panel's
  Spec in response, no re-render IPC happens.
- The plugin can still drive widgets imperatively when needed:
  `editor.widget(key).setValue(s)` is a single command, processed in
  `process_async_messages` like every other plugin command
  (`crates/fresh-editor/src/app/mod.rs:101`).

Lifecycle alignment with `editor_tick`:

1. Plugin event handler runs (e.g., `mouse_click` or a debounce
   timeout).
2. Plugin updates its model and calls `editor.setPanelSpec(panelId, spec)`.
3. Spec is queued as `PluginCommand::SetPanelSpec`.
4. Next `editor_tick` processes the queue (`process_async_messages`),
   diffs against the panel's previous Spec, applies the minimum widget
   tree mutation, marks `needs_render = true`.
5. `render.rs` paints the next frame.

This is the same cadence the imperative API already uses; we are not
adding a new tick or a new render path.

---

## 7. Theming

Widgets carry **roles**, never colors. A `Button` with
`kind: "danger"` resolves at render time to
`Theme::action_danger_fg/bg/hover_bg` (we add these keys; they live
alongside the existing 200+ in `crates/fresh-editor/src/view/theme/types.rs:1116`).
Plugin-side overrides are limited to:

```
spec.theme = { "Button.danger.fg": "#ff4400" }   // RGB or another role key
```

— a per-spec map, validated by the host (unknown keys logged, dropped).
This preserves `OverlayColorSpec` semantics
(`fresh.d.ts:600–634`) and routes through `resolve_theme_key` so
high-contrast and color-blind themes Just Work. Plugins that today
hard-code `"syntax.keyword"` for unrelated UI affordances stop doing
that; the migration plan converts the worst offenders.

Crucially, the plugin **cannot** ship its own Theme; it can only
override roles within a panel. The active Theme is always the user's.

---

## 8. i18n

Widgets carry **default English labels** (`Confirm`, `Cancel`,
`Replace All`, `Toggle`, `Expand`, plus aria/screen-reader strings).
The plugin overrides per-instance via props:

```
{ kind: "widget", type: "Button", props: { label: t("replaceAll") } }
```

We do **not** invent a widget-level i18n manifest; per-plugin
`*.i18n.json` (`docs/i18n.md`) stays the authority. The library ships
its built-in defaults in a single `lib-widgets.i18n.json` so the
HintBar's "Tab to next field" string is translatable without touching
any plugin.

---

## 9. Accessibility

Required for v1:

- High-contrast themes flow through naturally because widgets use
  roles. We add an explicit
  `theme.accessibility.high_contrast = true` resolution path the
  widget renderer reads to bump borders and disable subtle hovers.
- Configurable keybindings: every widget action is a named command
  (`tree.toggleExpand`, `list.activate`, `tabs.next`). Users rebind in
  `keybindings.json` against the existing `KeybindingResolver`. Plugins
  do not redefine these.
- Screen-reader output via OSC 52 / IDE bridges: every widget has an
  `aria` string the host emits on focus change and on event. We add a
  `view/accessibility.rs` that consumes widget focus changes and emits
  the appropriate OSC-52 / IDE-bridge messages; this is the same place
  that already serializes selection for clipboard (so we don't fork
  the OSC path).
- Motion-reduction: animations are role-keyed. The library has
  exactly two animations (focus-flash, hover-fade); both are gated on
  `theme.accessibility.reduce_motion`.

Nice-to-have (deferred):

- A full ARIA-tree model (parent/child/level-of). v1 ships
  flat live-region announcements per focus change.
- Live-region throttling (we throttle at one announcement per 100 ms
  to avoid drowning a screen reader during typing).

---

## 10. Migration plan: `search_replace.ts`

`search_replace.ts` is the densest widget user (~1305 lines).
Migration in five passes; no flag day:

### Pass 1 — Mount as a `Panel`, body stays `RawBuffer`

The plugin keeps emitting its existing `TextPropertyEntry[]` but
through a `RawBuffer` widget mounted inside a `Panel`. The HintBar
moves to a real `HintBar` widget. The toolbar of toggles
(case/word/regex) becomes real `Toggle` widgets. Tab cycling between
the toolbar and the body is now host-owned. Net diff: ~150 LOC moved
out of plugin, no functional change. **This validates the panel /
widget infrastructure end-to-end on the most demanding plugin
without touching the parts that are subtly broken.**

### Pass 2 — Replace search/replace fields with `TextInput`

`buildFieldDisplay` and `cursorPos` byte-offset math
(`search_replace.ts:557–565`) delete entirely. Cursor management
becomes the host's. `onChange(value)` is the plugin's new event.
**This is where the per-keystroke IPC saving lands.** Closes §11
(paste support) and §11 (history persistence) as widget-shaped: a
`TextInput` ships a paste handler and an optional history ring; both
are widget props.

### Pass 3 — Replace match list with `Tree`

The hand-rolled `FlatItem` array (`search_replace.ts:249–268`),
expand/collapse arrow handlers (`search_replace.ts:1006, 1037`), and
per-row checkbox rendering (`search_replace.ts:1147–1151`) all delete.
The plugin supplies a `Tree` provider:
`{ getChildren(key), getItem(key) → { label, checked?, badge? } }`.
**This unblocks §4 (mouse expansion of files) — clicking the
disclosure glyph is now a `onToggleExpand(key)` event, and §14
(multi-line match-list rendering) — `Tree` items can be multi-row
because the layout knows how to size them.**

### Pass 4 — Glob filter input as `TextInput` with validator

Closes the remaining §11 item.

### Pass 5 — Delete dead code

The flatten/index plumbing, the byte-offset cache rebuild, the
focus enum, and the keymap entries that the host now owns all go.
Conservative estimate: 400-600 LOC out of `search_replace.ts`.

Each pass ships independently; each one is reviewable; the plugin
keeps working between them.

---

## 11. Smallest first PR

**Title**: `widgets: introduce Spec/Panel scaffolding and migrate
search_replace HintBar`

**Diff shape (no code yet, just the surface that lands)**:

- New Rust file `crates/fresh-editor/src/widgets/mod.rs` —
  `Spec` enum, `WidgetTree`, `Reconciler`, `WidgetHandle`. Re-exports
  the existing `view/controls/*` `*State` types; `render_*` functions
  are the renderers.
- New Rust file `crates/fresh-editor/src/widgets/dispatch.rs` — hooks
  into `app/event_dispatch.rs` for hit-test (extends the dispatcher
  proposed in `event-dispatch-architecture.md` Phase 2).
- New `PluginCommand` variants in `crates/fresh-core/src/api.rs`:
  `MountPanel { panel_id, spec }`, `UpdatePanel { panel_id, spec }`,
  `UnmountPanel { panel_id }`, plus the inverse events
  (`WidgetEvent::Activate { panel_id, key }`, `Toggle`, `Change`,
  `Submit`, `Hover`).
- New TS file `crates/fresh-editor/plugins/lib/widgets.ts` exporting
  `mountPanel`, `Spec`, helpers `Row`, `Col`, `Button`, `Toggle`,
  `Tree`, `List`, `TextInput`, `HintBar`, `RawBuffer`. ~300 LOC,
  declaration only.
- Update `fresh.d.ts` with the new commands and event payloads.
- Migrate **only** `search_replace.ts`'s HintBar to use the `HintBar`
  widget (Pass 1 partial). Lines deleted: ~30. Lines added: ~10.
- One integration test under `crates/fresh-editor/tests/` that spawns
  a stub plugin which mounts a `Panel { HintBar }` and asserts the
  rendered output — this is the test infrastructure every subsequent
  widget needs.

The PR is small enough to land cleanly, exercises the full IPC path
(mount, render, event, unmount), and changes one user-visible thing
(HintBar) so reviewers can verify in tmux.

---

## 12. Prior art — what we steal, what we reject

| System | Steal | Reject | Why |
|---|---|---|---|
| **VS Code TreeView** | Declarative `TreeDataProvider` shape: plugin returns data, host owns hit-test, virtualization, focus | Webview as a generic UI escape hatch — every webview is an extension-authored XSS sink with `postMessage` privilege | Webviews break the sandbox premise; TreeView's declarative shape is exactly the v1 widget-spec model |
| **Helix `Component` trait** | Layered z-ordered components; bubble-up `Consumed | Ignored`; host-owned `cursor()` and `required_size()` | Synchronous Rust trait across FFI | Translation: TS handlers are async with timeout; `Ignored` is the IPC default; the *protocol* survives |
| **nui.nvim** | Widget = "buffer + keymap + lifecycle (mount/unmount)" | "No widget library" stance | TS plugin authors are not Vimscript veterans; sandboxed JS plus opinionated widgets is a better default |
| **Sublime minihtml** | `on_navigate` href dispatch as the safe link primitive (already analogous to `mouse_click`) | HTML/CSS layout subset; no keyboard focus | We need real keyboard widgets, and CSS-flow on a terminal is the wrong fit |
| **Emacs widget.el** | Nothing | The whole library | The well-known critique (resists composition, imperative-by-side-effect) is exactly what we'd reproduce by exposing today's `setVirtualBufferContent` as the only model |
| **Magit transient.el** | Grouped key→command menu as a first-class widget | Lisp-y EIEIO subclassing | A `Transient` widget with `{groups: [{title, entries: [{keys, label, command}]}]}` covers `git_log` and unblocks discoverability per `plugin-usability-review.md` |

---

## 13. Risks and rejected alternatives

### Rejected alternatives

- **TS-only library.** Section 1.
- **Replace `setVirtualBufferContent` outright.** Forces a flag-day
  rewrite of 107 plugins. Rejected for backwards compat alone.
- **Imperative widget handles only (no declarative Spec).** Considered.
  Plugins would call
  `editor.createButton({label}).onActivate(...).mount(parent)`. This
  is the React-without-JSX model. Rejected because every plugin would
  re-implement reconciliation by hand: `if (currentLabel != newLabel)
  button.setLabel(newLabel)`. The Spec/reconciler model centralizes
  this.
- **An `iframe`-equivalent (Webview) component.** Rejected on the same
  grounds VS Code itself documents — the security cost dominates the
  flexibility benefit, and we have zero of VS Code's CSP and process
  isolation infrastructure.

### Risks

| Risk | Mitigation |
|---|---|
| Reconciler complexity grows past what one engineer can hold | Keep Spec flat (no nested per-widget keys beyond `key: string`); cap recursion depth; ship the dirtiest plugin (`search_replace.ts`) as the regression test for every reconciler change |
| Per-keystroke event IPC still dominates if plugins re-emit Spec on every keystroke | Document the rule: in `onChange`, never re-emit Spec unless model state actually changed. The lint is "panel.update calls per second"; expose it on the dev HUD. |
| Capability creep through widget callbacks | Widgets only emit *events* the plugin can already subscribe to. Code review checklist item: a new widget MUST NOT introduce a new `PluginCommand`-equivalent capability. |
| Theme role explosion (`Button.danger.hover.fg`...) | Cap the role tree at three levels; review additions in PRs that touch `theme/types.rs` |
| Reach: Settings doesn't actually adopt the widget tree | Keep the *renderers* shared (`view/controls/*::render_*`) and the *Spec* shape compatible. Settings can live on its current direct calls indefinitely; if/when it migrates, the renderers do not move. |
| Plugin author confusion: Spec vs imperative | One way per use-case in the docs. `RawBuffer` exists for *escape hatches*, not for rendering rich UI. |
| Terminal-constraint violations (Shift+Enter etc.) | Static lint in TS: any `keys` string in a `HintBar` or `Transient` matching `^Shift\+(Enter|Alt\+Enter)` is a build error. |
| Drift from the four open plans (`UNIFIED_UI_FRAMEWORK_PLAN`, `unified-hit-test-theme-plan`, `unified-keybinding-resolution`, `event-dispatch-architecture`) | This proposal explicitly builds on them. Land the open dispatcher work *before* migrating Pass 2/3 of `search_replace.ts`. |

---

## 14. Order of landing (Rust-side)

1. `event-dispatch-architecture.md` Phase 2 (`hit_test(col, row)`
   dispatcher) — required by §5.
2. `unified-hit-test-theme-plan.md` `region_at` extension — adds
   widget regions to the dispatcher.
3. `unified-keybinding-resolution.md` collapse — required by §4.
4. `crates/fresh-editor/src/widgets/{mod,dispatch}.rs` — new module,
   re-uses `view/controls/*State`.
5. `crates/fresh-core/src/api.rs` — new `PluginCommand` variants
   (mount/update/unmount + events).
6. `crates/fresh-editor/plugins/lib/widgets.ts` — TS surface.
7. Smallest first PR (§11).
8. Pass 2–5 of `search_replace.ts` migration.
9. Settings migration to the same renderers (paid down opportunistically).
10. `view/settings/` adopts `Spec` for parts it owns (optional;
    the renderers are already shared).

---

## 15. Go / don't go

**Go.** This is a hybrid widget runtime that finishes the job
`UNIFIED_UI_FRAMEWORK_PLAN.md` started: Rust owns layout, hit-test,
focus, theming, and widget state; TypeScript plugins emit a
declarative Spec and consume semantic events; the existing
`setVirtualBufferContent` and `defineMode` primitives stay so all 107
plugins keep working unchanged on day one. It grants no new
capabilities, makes high-contrast and i18n actually consistent,
unblocks the §4/§11/§14 follow-ups in `search_replace.ts` as concrete
widget features, and lets the Settings pane and plugin widgets share
renderers — which is the original motivation for the unification work.
The smallest first PR (HintBar migration) is small, reversible, and
exercises the full IPC path; if it doesn't feel right after a week,
the only thing on the floor is one new module and one HintBar. Land
it.
