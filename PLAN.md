# PlanCraft — plan

> **Status:** Phase 3b (the RevuCraft link, PlanCraft side), v0.7.0: RevuCraft-style chrome (menu bar, Plan/Read mode bar, status bar, right-hand rail), RevuCraft themes with a choice of accent, and tool icon colours. Before it: v0.6.1 (the Start menu shortcut fix, the RevuCraft link in a Settings panel, linked drawings with unlink, punch items as tasks). Calendar and Board views (v0.5.0), Phase 3 (v0.4.0), Phases 0–2 (v0.1.0–v0.3.0).
> **Repo:** `Azurehawke/plancraft` (created empty; holds only this plan until work starts).
> **Base:** GridCraft, to be imported from [`Azurehawke/gridcraft`](https://github.com/Azurehawke/gridcraft) (upstream [`storytold/gridcraft`](https://github.com/storytold/gridcraft), MIT OR Apache-2.0).
> **Last updated:** 2026-11-05

## Goal

A local-first, MS Project-style project management app in Rust, built on GridCraft's engine and UI.
It runs locally on every device (Windows laptop, Surface Pro 7s, Galaxy Tab, Pixel phone) and
saves projects into a shared folder synced by Syncthing. The same project can be edited on several
devices, including offline, and the edits merge cleanly.

## Suite scope (proposed 2026-11-05)

PlanCraft is the **hub of the Craft suite**; RevuCraft and the other apps are **spokes** (decided
2026-11-05). The proposals below are not built. Nothing here changes the phases already done.

### Principles

- **Portable.** Everything about a project lives in its folder: documents, budgets, ledger,
  receipts, mail, forms, messages. Copying or syncing the folder moves the whole project.
- **Events and messages are files.** Each mail message, receipt, ledger entry, form response or
  team message is its own file, so Syncthing syncs them without conflicts.
- **Original templates.** Templates based on PMBOK-style documents use our own structure and
  wording. Names of general concepts (project charter, risk register) are fine; PMI's text is not
  copied.
- **Not legal or tax advice.** The ledger and signatures are tools; compliance rules come from the
  user's accountant or lawyer.

### Proposed areas

1. **Full project lifecycle.** From initiation to closing: risk, issue, change and stakeholder
   registers, lessons learned. Phase 5b's document work sits inside this.
2. **Documents and templates (most immediate; see [Documents and templates](#documents-and-templates)).**
3. **Budgets and finance.** Budget sheets in GridCraft spreadsheets. A project ledger with a chart
   of accounts, receipts attached to entries, and a mini accounting model: double-entry, whole
   cents, an audit trail, and closed periods. Open: does this live here or in CraftStudio's planned
   financial module?
4. **Project mail.** Filing and sending, including documents sent from a template and progress
   report requests for the Gantt. The scope is the project mail described above; a fuller client
   is a later decision.
5. **Forms.** Build and send forms (based on PdfCraft's forms), collect responses, and sign for
   approval. Signature rules depend on identity, consent and audit trail, and must be checked for
   the jurisdictions used before a signature counts as binding.
6. **Team messages (deferred).** Start with messages stored as files in the project folder, as
   with mail. Real-time chat needs a relay server, which is a separate decision and is not in scope
   now.
7. **Collaboration.** As built: file sync through Syncthing with Loro (Phase 1). Live presence is
   not designed.
8. **RevuCraft.** The link as built (Phase 3b, one-way punch items and status) stays as it is. A
   two-way exchange, such as completed tasks flowing back from RevuCraft, is a later step. It needs
   a versioned message format, a rule for which app owns each field, stable IDs across both apps,
   event files that are safe to apply twice and out of order, and a queue for when either app is
   closed.

### Immediate focus

Documents and templates come first when work starts. Decisions needed before building:

- **Format:** Markdown templates first (matching the Documents tab), with .docx export later. This
  replaces the .docx / WordCraft bridge below as the first version, if agreed.
- **Field syntax** and how repeating tables are marked (open question below).
- **First template list:** the charter and status report first, then the rest.
- **Originality check** for each PMBOK-style template, written from our own outline.

## What GridCraft already provides

| GridCraft piece | Reuse in PlanCraft |
|---|---|
| Command architecture (`Session::execute`, `crates/engine/src/cmd/*`) | Every project action is a command, so undo, never-crash rollback, CLI and MCP come free |
| Undo groups (`Session::grouped`) | Multi-step edits (drag a bar that reschedules successors) as one undo step |
| Virtualised grid (`ui-egui/grid.rs`) | Task sheet, Resource sheet, Usage views |
| Row outlining (`engine/outline.rs`, `ui-egui/outline.rs`) | Starting point for WBS summary tasks (indent/outdent, collapse) |
| Ribbon widgets, theme tokens, panes | Project ribbon (Task, Resource, Report, Project, View tabs), Format panes |
| Formula engine + date functions (`functions/src/date.rs`: WORKDAY, NETWORKDAYS…) | Custom fields with formulas, like MS Project's; reference for working-day math |
| Number formats (`numfmt`) | Durations, dates, costs, percentages |
| `chart` crate | Report charts (burndown, cost, resource histograms) |
| `pdf` crate | Print and PDF export of Gantt and reports |
| `xlsx` crate | Export task/resource tables to Excel |
| Layering enforced by `cargo xtask layers` | New crates slot in at the right layer |

## What needs to be built

### New crates

| Crate | Layer | Purpose |
|---|---|---|
| `project-model` (`plancraft-project`, built) | L2 | Tasks, WBS outline, dependencies, calendars, resources, assignments (built); baselines, custom fields (later phases) |
| `schedule` | L2 | Critical path scheduler: forward/backward pass, slack, constraints, summary rollups; later resource leveling |
| `project-sync` (`plancraft-project-sync`, built) | L2 | CRDT document (Loro), per-device change files, merge on open, reload of changed files (live file watching comes with the app's views) |
| `mspdi` | L2 | MS Project XML (MSPDI) import and export |
| Project commands | L5 | `project.*` / `task.*` / `resource.*` / `baseline.*` command families in the engine |
| Project views | L6 | Gantt, Network Diagram, Resource Sheet, Task Usage, Resource Usage, Tracking Gantt, Calendar |

### Data model (first draft)

- **Task:** id, name, WBS code, outline level, summary/milestone flags, duration, work, start,
  finish, constraint type + date, deadline, % complete, actual start/finish, priority, notes,
  calendar override, custom fields.
- **Dependency:** predecessor, successor, type (FS / SS / FF / SF), lag (duration or %).
- **Calendar:** working week, working hours, exceptions (holidays, overtime days); project,
  resource and task calendars.
- **Resource:** work / material / cost, max units, rates, calendar, group.
- **Assignment:** task × resource, units, work, cost, contour.
- **Baseline:** up to 11 saved snapshots of start/finish/duration/work/cost.

### Scheduler

1. Forward pass (early start/finish) and backward pass (late start/finish) over the dependency graph.
2. Calendar-aware duration math (working time, not elapsed time; elapsed durations as an option).
3. Constraints: ASAP, ALAP, SNET, SNLT, FNET, FNLT, MSO, MFO.
4. Summary task rollups; total and free slack; critical path.
5. Tracking: actuals and % complete push remaining work.
6. Later: resource leveling (TaskJuggler's behaviour is the reference).

**As built (Phase 2, `crates/schedule`):** whole working days (durations ÷ hours per day, rounded
up); FS/SS/FF/SF with lag or lead; links to or from a summary apply to every task under it;
SNET/FNET/MSO/MFO in the forward pass, SNLT/FNLT/MSO/MFO and deadlines in the backward pass; ALAP
placed at late dates; total slack and critical tasks; summaries span their subtasks; cancelled
tasks skipped; link loops left out and reported; bounded against hostile values. Dates are
computed, never stored, so every device shows the same schedule without sync traffic. Not yet:
hours within a day, elapsed durations, task and resource calendars, actuals pushing remaining work
(tracking, Phase 5), free slack, leveling.

Test-first: hand-checked schedules as fixtures, plus MSPDI files produced by MS Project or MPXJ
used only as comparison oracles (never committed if made by MS Project, per the clean-room rules).

### Views

- **Gantt Chart:** task table on the left, timescale with bars on the right; drag to move,
  resize, link; zoomable timescale; critical path highlight; baseline bars.
- **Network Diagram, Resource Sheet, Task Usage, Resource Usage, Tracking Gantt, Calendar.**
- **Phone/tablet layout:** follows the conventions of the other Craft Android ports.

## Interface

**Decided (2026-10-27): Obsidian-style shell**, with ribbons that change by the kind of tab in focus.

### Layout

- **Left sidebar with switchable panels** (like Obsidian's): an icon strip picks the panel, and
  the whole sidebar collapses to that strip (toggle) to give the Gantt full width. See
  [Sidebar panels](#sidebar-panels).
- **Top of centre: document tabs.** Several items open at once; tabs can be split side by side
  (e.g. Gantt next to the sprint plan or risk register).
- **Centre: the open item** (Gantt, board, document, sheet…).
- **Ribbon above the centre pane, by tab type:**

| Tab type | Ribbon |
|---|---|
| Schedule | Task, Resource, Project, View tabs (MS Project-style) |
| Board / backlog | Sprint and story tools |
| Document | Template-filling tools, "Edit in WordCraft", "Import into project" |
| Sheet | GridCraft's normal ribbon |

### Sidebar panels

Each panel is its own tab in the left sidebar, chosen from the icon strip; panels can be
reordered, hidden, or stacked (two panels split vertically), as in Obsidian.

**From Obsidian**

| Panel | What it does in PlanCraft |
|---|---|
| Explorer | The project folder as it is on disk: schedule, backlog, board, `docs\`, attachments |
| Search | Full-text search across tasks, notes, stories, risks, documents and filed mail in the project; results open the item |
| Bookmarks | Pinned items: a task, a view (e.g. "Critical path, next 2 weeks"), a document, a filter |

**Project-specific panels** (decided 2026-10-27)

| Panel | What it does |
|---|---|
| Outline / WBS | The task tree; click to jump to a task in whatever view is open; drag to re-parent |
| Filters / saved views | Saved filters and groupings (critical, late, by resource, this sprint) |
| Recent | Recently opened items and recently changed tasks, including changes synced from other devices |
| Sync | Which devices have contributed changes and when, plus any document lock notes |

**Right sidebar: Inspector** (decided 2026-10-27). Like Obsidian's properties pane: shows and
edits the properties of the selected task, story, resource or document; collapsible like the
left sidebar, and part of the flyout layout on phone and tablet.

### Views inside a schedule tab

A schedule tab has a small view switcher in its corner (Affinity-style: same data, different
toolbar): Gantt, Tracking Gantt, Network Diagram, Resource Sheet, Task Usage, Resource Usage,
Calendar, Kanban.

### Phone and tablet

- The sidebars (left panels and Inspector) become flyouts (as in RevuCraft's phone layout); tabs become a switcher.
- Same layout scales from the Pixel to the Galaxy Tab and desktop.

### CraftStudio path

The same shell could later hold drawings, schedules and documents as different tab types, which
is close to the mode switching planned for CraftStudio.

## New project wizard

**Decided (2026-10-29):** an **optional** wizard for the common project information when starting
a new project. "Skip" at any step creates the project with whatever has been filled in (or blank).

### Steps (first draft)

1. **Basics:** title, purpose / description, client, site address
2. **Dates:** start date, deadline (finish), working calendar (work week, holidays)
3. **People:** project manager, sponsor, stakeholders (name, role, contact), team
4. **Budget:** total budget, currency, optional cost categories
5. **Method:** waterfall, agile/scrum or hybrid; picks the starting views and template set
6. **Review:** summary of everything entered, then Create

### Where the answers go

- Stored as **project properties** in the project file (synced like everything else).
- Fill the matching fields in document templates (charter, scope, status report) and the
  stakeholder register.
- Editable later in a **Project Information** pane (and the Inspector when the project itself is
  selected), so skipping the wizard loses nothing.
- Same wizard reachable as a command (`project.newWizard`), and its fields settable through the
  CLI / MCP like every other command.

## RevuCraft link (optional)

**Decided (2026-11-01):** PlanCraft is a **standalone app** for any kind of project. Linking to
RevuCraft is an **option in Settings**, off by default; nothing in the core app depends on it.
Details and the RevuCraft side are in [`docs/revucraft-integration.md`](docs/revucraft-integration.md).

### What the link does

- **RevuCraft → PlanCraft:** PlanCraft reads punch items and comments from linked drawings and
  works them into the plan, progress and Gantt automatically.
  - *Example:* a punch item added in RevuCraft for a tech, with an assignee and due date, appears
    in PlanCraft as a task assigned to that tech with that due date, without retyping.
  - Punch status drives the task's progress (Closed = done); comments and replies become task notes.
- **PlanCraft → RevuCraft:** RevuCraft reads the plan (tasks, assignments, dates, dashboard) so
  drawings, assignments and the schedule stay in step.

### How it works (first draft)

- **Settings:** a global "Link to RevuCraft" switch; per project, the drawings (PDFs in the job
  folder) to watch.
- **New punch item → new task**, placed under a "Field items" summary task (or a parent chosen in
  settings). Assignee matched to a resource by name, or a new resource created; due date becomes
  the task's deadline/finish.
- **Field ownership:** punch fields (description, assignee, due date, status) are edited in
  RevuCraft and shown in PlanCraft with "Edit in RevuCraft"; scheduling fields (WBS position,
  dependencies, duration, baselines) are PlanCraft's.
- **Punch deleted in RevuCraft:** the task is marked cancelled, not deleted, so history is kept.
- **Transport:** RevuCraft's `markups.json` sidecar (per the integration notes), or RevuCraft
  writing its own Loro change file in the project's `devices\` folder now that Loro is chosen.
  To be settled with the RevuCraft session.
- Stable task ids (not WBS codes) for links in both directions.

**As built (Phase 3, `crates/commands`, `crates/shell`):**
- `plancraft-commands`: every project action is a command id with JSON parameters (`task.add`,
  `task.indent`, `task.link`, `project.setInfo`…), used by the ribbon, Gantt, Inspector and wizard.
- `plancraft-shell`: tab strip; project ribbon (Project, Task, Schedule, View groups); left icon rail
  with Explorer, Search, Bookmarks, Outline, Filters, Recent, Sync; right Inspector (task
  properties, links, resources, notes; Project Information when no task is selected); Gantt view
  (task table with WBS, in-place editing, collapse; timescale with working/non-working days,
  critical path, progress, links, milestones, today line; drag a bar to move it — sets "start no
  earlier than" — or its end to change the duration; zoom and scroll); keyboard (Insert, Delete,
  F2, arrows, Alt+Shift+←/→); the optional new project wizard (six steps, Skip and create;
  stakeholders become resources).
- The desktop app opens in the shell; spreadsheets open in a Spreadsheet tab (GridCraft's UI, as
  before). `.plancraft` files open PlanCraft on double-click (Windows installer); other devices'
  saves are picked up every 2 s; bookmarks and recent projects are kept per device (`shell.json`).
- Not yet: the other views in the view switcher (Tracking Gantt, Network Diagram, Resource Sheet,
  usage views), split tabs, document templates in the Explorer, multi-row drag.

**Calendar and Board views (built 2026-11-01, v0.5.0):**
- **Calendar:** a month (Monday first) with each task as a bar across the days it runs, wrapping
  onto the next week; milestones as diamonds; critical tasks red; non-working days shaded; today
  ringed; "+n more" when a day is full. Previous/next month, Today, Project start; summary tasks
  optional. Click a bar for the Inspector; drag it to another day (sets "start no earlier than");
  double-click a day to add a task starting there.
- **Board (Kanban):** one card per task (summaries left out) in the project's board columns
  (default Backlog, To Do, In Progress, Review, Done; renamed per project, 2–12 columns, the last
  means done). Cards show WBS and summary, name, due date (red when late), people (initials in
  their colour), progress, and a red edge on the critical path. Drag a card between columns
  (`task.setStatus`): moving into the last column sets 100%, moving out of it from 100% sets 0%.
  "Show" limits the board to one summary task; "+ Add card" adds a task into a column.
- Model: tasks have a `status` (board column); a task without one sits in the column its progress
  suggests (not started: "To Do", started: "In Progress", 100%: the last column). Board columns
  live in the project (`board` map), so every device sees the same board.
- Still for the agile phase (5b): sprints, story points, WIP limits, swimlanes, card order within
  a column, burndown.

## Multi-device design

**Problem.** If two devices edit one file and Syncthing syncs it, Syncthing keeps one and writes
`.sync-conflict` copies, so one device's work is effectively lost.

**Approach.** Each project is a folder; each device writes only its own file:

```
M:\Jobs\Acme-Network\Acme-Network.plan\
    Acme-Network.plancraft   # entry file: double-click / Obsidian opens the project (id, name, format version)
    devices\
        laptop.loro          # this device's CRDT changes (only the laptop writes it)
        surface-1.loro
        galaxy-tab.loro
    docs\                    # .docx documents made from templates (+ lock notes)
    mail\                    # .eml messages filed to the project
    attachments\             # photos, linked PDFs (optional)
```

- On open: load every file in `devices\`, merge into one CRDT document, build the model.
- On save: append this device's new changes to its own file only.
- While open: watch the folder; when Syncthing delivers another device's file, merge it live.
- Conflicting edits to the *same* field resolve deterministically (CRDT rules); edits to
  different fields or tasks always merge.
- Occasionally compact: a device may write a merged snapshot, after which old change files can be trimmed.

**Sync library: Loro** (MIT, Rust; decided 2026-10-31). Chosen over Automerge because:

- **Movable tree:** the WBS (indent, outdent, drag to re-parent) is a built-in type; concurrent
  moves on two devices cannot duplicate a task or create a parent loop.
- **Movable list:** reordering tasks and backlog items merges without duplicates.
- **Version history:** the project can be opened as it was at any earlier point (e.g. before a
  client change).
- **Shallow snapshots:** used for the compaction step above.
- Runs in WebAssembly (Obsidian plugin) and on Android.

**Risk:** Loro is younger than Automerge (1.0 in October 2024) with a smaller community.
**Phase 1 prototype test:** a few thousand tasks, edited offline on the laptop and the tablet with
conflicting moves and edits, synced by Syncthing; must merge correctly in both the native app and
the WebAssembly build, with acceptable file sizes and load times. If it fails, switch to Automerge
(MIT) before anything else depends on the sync layer; Automerge would need app-level code for
moves and tree-loop repair.

**Prototype result (2026-11-01): passed; Loro stays.** `crates/project-sync/tests/merge_prototype.rs`
(Loro 1.17.1): a 3,110-task job (3,000 tasks, 110 summaries, 3,000 links, 3,000 assignments)
edited offline on three devices with deliberate clashes, then synced Syncthing-style in
different orders and partially:

| Clash | Result |
|---|---|
| Crossed moves (X under Y here, Y under X there) | One move wins; never a loop |
| Same field, different values | One value wins, the same on every device |
| Different fields of one task | Both edits kept |
| Delete a summary here, edit/add under it there | The delete wins (subtasks go with it) |
| Same tasks reordered differently | No task duplicated or lost |
| 50 tasks added at the same place on each device | All 150 kept, in one order everywhere |
| Crossed links / link to a task deleted elsewhere | Kept and reported as problems (loop, dangling link) |
| Plus 1,500 seeded random edits per device | All three devices end identical |

Merging every order of the device files gives the same project; a half-synced file is skipped and
picked up on the next reload. Numbers (release build): open + merge three device files ≈ 0.14 s;
each device file ≈ 970 KB for the 3,110-task job with its full edit history.
WebAssembly: the crates are checked for `wasm32` in CI (`cargo xtask wasm`); running the merge
tests inside a browser waits for the Obsidian plugin phase.

**As built (Phase 1):** each device's file is a full Loro snapshot of everything that device knows
(any one file rebuilds the project; files are merged, never overwritten by another device). Every
session edits as a fresh random Loro peer, so two windows on one device can't be confused for one
editor; before saving, a device re-reads its own file so a second window's save isn't lost.
Writes go to a `.syncthing.*.tmp` file then are renamed into place (Syncthing ignores those names).
Compaction (shallow snapshots) is left for when files grow.

The same pattern could later be backported to RevuCraft if it uses one shared file.

## Documents and templates

> **Open:** the WordCraft / .docx bridge below is the earlier plan. A Markdown-first version is
> proposed under [Suite scope](#suite-scope-proposed-2026-11-05) and is not yet decided.

PlanCraft owns the project data; documents are typed in WordCraft (which reads and writes
.docx and already has templates, merge fields and mail merge). .docx is the bridge between them.

### Data flow

- **Project → document.** Creating a document from a template fills its fields from the live
  project: name, sponsor, dates, milestones, team, including repeating rows (e.g. a milestone
  table). The narrative is then written in WordCraft.
- **Document → project.** Tables with known headings are read back into the project with
  "Import into project": a WBS table becomes tasks, a user-story table becomes backlog items,
  a risk table becomes the risk register.
- **Where content lives.** Narrative documents (charter, scope, retrospective notes) are
  written in WordCraft. Long lists that are sorted and filtered (backlog, risk register,
  stakeholders) live in PlanCraft's grid, with a template that prints or exports them as a document.

### Template set (first draft)

**Waterfall**
- Project charter
- Scope statement / requirements
- WBS and WBS dictionary
- Stakeholder register and RACI matrix
- Communication plan
- Risk register
- Change request form
- Phase gate / sign-off
- Status report
- Closeout and lessons learned

**Agile / Scrum**
- Product vision
- Product backlog (epics → user stories)
- User story card ("As a… I want… so that…", plus acceptance criteria)
- Definition of Ready and Definition of Done
- Sprint planning (goal and committed items)
- Daily stand-up log
- Sprint review
- Sprint retrospective
- Release plan

**Shared:** meeting minutes, status report (variants for each method).

**Job-specific** (same mechanism): site survey, statement of work, punch list,
handover / as-built acceptance.

### Agile data model

Scrum templates need agile work modelled in the app, not only a Gantt:

- Backlog with epics and user stories, story points, acceptance criteria
- Sprints (goal, dates, committed items)
- Kanban board view
- Burndown / burnup charts (through the `chart` crate)
- **Hybrid projects:** one project can mix both methods, e.g. a waterfall install schedule where
  one phase runs as sprints and shows on the Gantt as a summary task.

### Storage and sync

- Templates ship with the app; a project's documents live in `docs\` inside the project folder
  and sync like everything else.
- A .docx is a single file, so the per-device merge approach does not apply: two devices editing
  the same document still produce Syncthing conflict copies. Documents are usually written by one
  person at a time, so a lock note in the folder (e.g. "open on Surface-1"), shown as a warning
  when another device opens the document, should be enough.

## Project mail

**Status: early idea (2026-10-29).** Look and behaviour not decided; this section is a starting
point, not a design.

### Direction

*Project mail*, not a full Outlook-style client: the mailbox stays on the mail server, and only
the messages that matter to a project are brought into it.

- **Mail panel** in the left sidebar and a **mail tab type** in the Obsidian-style shell.
- **File to project:** drag a message onto the project to save it as an `.eml` in `mail\` inside
  the project folder. Each message is its own file, so Syncthing syncs it without conflicts.
- **Link and create:** link a message to a task, or turn it into a task or punch item; the
  Inspector shows a task's linked mail.
- **Send from the project:** status reports and templates sent with project fields filled in.
- **Search:** filed mail appears in the Search panel with tasks and documents.
- **Accounts per device:** credentials live in each device's own secure storage (Windows
  Credential Manager, Android Keystore), never in the shared folder.

### Libraries (permissive, usable directly)

| Library | Licence | Use |
|---|---|---|
| Pimalaya crates (behind the [Himalaya](https://github.com/pimalaya/himalaya) CLI) | MIT or Apache-2.0 | IMAP, SMTP, JMAP, Gmail API, Microsoft Graph, Maildir/mbox; OAuth tokens supported, but the sign-in flow must be added |
| Stalwart `mail-parser`, `mail-builder`, `mail-send` | Apache-2.0 or MIT | Read filed `.eml` files, build and send messages |
| `lettre` (SMTP), `async-imap` (IMAP) | MIT / Apache (to confirm) | Alternatives for sending and IMAP |

### Reference only (GPL)

| Project | Licence | What to learn |
|---|---|---|
| meli | GPL-3.0-or-later | Full-featured terminal client: threading, search, account handling |
| Penguin Mail | GPL-3.0 | Native Rust desktop client with mail, calendar, contacts, OpenPGP: UI ideas |

## Obsidian plugin

**Status: later phase (2026-10-29).** PlanCraft should also run as an Obsidian plugin, like the
GridCraft plugin (`Azurehawke/obsidian-gridcraft`): the engine compiled to WebAssembly with an
Obsidian-native UI, installed through BRAT.

### How it works

- Obsidian supplies the shell: file explorer, search, bookmarks, tabs.
- The plugin adds the project views as Obsidian view types: Gantt, board/backlog, resource sheet,
  Inspector.
- **Embeds:** `![[Acme.plan#Gantt]]`, `#Dashboard` etc. show a live, read-only view in a note;
  editing happens in the tab (same rule as the GridCraft plugin).

### Design rules (apply from phase 1, so the plugin is not a rewrite)

1. **Core crates stay UI-free.** `project-model`, `schedule` and `project-sync` must not depend on
   egui or any UI, so they compile to WebAssembly. A wasm build check runs in CI from phase 1.
2. **Sync library runs in WebAssembly.** Loro has a wasm/JS build; the phase 1 prototype test
   covers it.
3. **Each project has an entry file.** Obsidian opens files by extension, not folders, so a project
   needs a file such as `Acme.plan\Acme.plancraft` (or a `.plancraft` file next to the folder).
   This also settles the folder extension and layout decision.
4. **Device ID is never stored in the vault.** Each install writes its own change file, so it needs
   a stable device ID. The plugin's `data.json` lives in `.obsidian\` and syncs with Syncthing, so
   the ID goes in local storage that does not sync; otherwise two devices would share an ID and
   overwrite each other's changes.

### What stays in the app

- **Mail:** Obsidian mobile cannot open mail-server connections. The plugin may show filed `.eml`
  messages; fetching and sending stay in the app.
- **.docx documents:** open in WordCraft through "Open in default app". Markdown versions of the
  templates suit the plugin and could be filled from the project inside Obsidian.

## Reference projects

GridCraft's rules forbid copying GPL/AGPL/LGPL code, and PlanCraft keeps GridCraft's
MIT/Apache licence. These are **behaviour references** (study, observe, test against), not code sources:

| Project | Licence | What to learn |
|---|---|---|
| ProjectLibre | CPAL | Closest MS Project clone: ribbon, WBS/RBS charts, earned value, MS Project import/export |
| meli, Penguin Mail | GPL-3.0 | Email clients; see [Project mail](#project-mail) |
| GanttProject (`bardsoftware/ganttproject`) | GPL | Gantt interaction, baselines, resource charts |
| TaskJuggler (`taskjuggler/TaskJuggler`) | GPL | Scheduling and resource leveling |
| MPXJ (`joniles/mpxj`) | LGPL-2.1 | MS Project field mapping; reads .mpp and ~20 other formats, writes MSPDI. Use as a test oracle / external converter only |

More reference repos to be added after further research.

**Code that can be used directly** (permissive): Loro (sync), `petgraph` (if a
separate task graph is wanted), Pimalaya and Stalwart mail crates (see [Project mail](#project-mail)), plus GridCraft itself.

## Phases

| # | Phase | Done when |
|---|---|---|
| 0 ✅ | Repo setup: import GridCraft's history into this repo, add `upstream`, rename app/binaries, keep shared crates untouched | Builds and runs as PlanCraft on Windows |
| 1 ✅ | Loro prototype test, then `project-model` + `project-sync` + folder format + entry file; wasm build check in CI | Same project edited offline on laptop and tablet, synced by Syncthing, merges cleanly |
| 2 ✅ | `schedule` crate (CPM, calendars, constraints) | Fixture suite passes |
| 3 ✅ | Obsidian-style shell (sidebar panels, Inspector, tabs, per-tab ribbon), new project wizard + Project Information pane, task sheet + Gantt view + `task.*` commands | Usable for a real network-install job |
| 3b ✅ | RevuCraft link: the switch, linked drawings, punch items → tasks (`markups.json`, format 1), status and progress from RevuCraft. Plan → RevuCraft (summary file) comes with the dashboard | A punch item added in RevuCraft shows up as an assigned, dated task on the Gantt |
| 4 | Resources, assignments, usage views | Resource over-allocation visible |
| 5 | Baselines and tracking | Tracking Gantt shows slip against baseline |
| 5b | Agile model (backlog, sprints, board, burndown) and document templates (fill + import back). Documents and templates come first when work starts (proposed 2026-11-05) | A sprint planned in PlanCraft, its planning doc typed in WordCraft, stories imported back |
| 5c | Budgets and finance: budget sheets (GridCraft), chart of accounts, ledger, receipts (scope to be decided: here or CraftStudio) | A month's receipts filed to the project, posted to the ledger, and the budget sheet shows actual against plan |
| 5d | Lifecycle registers: risks, issues, changes, stakeholders, lessons learned | A risk logged, linked to a task and reported in a status document |
| 6 | MSPDI import/export; XLSX and PDF reports | A project round-trips through MS Project XML |
| 7 | Resource leveling, Network Diagram | — |
| 7b | Project mail (scope to be decided) | A message filed to a project, linked to a task, and visible on another device |
| 7c | Forms: build, send, collect responses, sign for approval | A form sent by mail, answered, and signed; the response filed to the project |
| 7d | Team messages as files in the project folder (real-time chat deferred to a server phase) | A message posted on one device appears on another after sync |
| 7e | RevuCraft two-way exchange (versioned messages, field ownership, stable IDs) | A task completed in RevuCraft is marked done in PlanCraft, and a rescheduled date reaches RevuCraft |
| 8 | Android build (Craft port conventions: self-update from repo, phone layout) | Runs on the Galaxy Tab and Pixel |
| 8b | Obsidian plugin (wasm engine, Obsidian-native views, read-only embeds; BRAT) | A project opens in an Obsidian tab and embeds in a note, on desktop and mobile |
| 9 | Optional: module inside CraftStudio | — |

## Upstream policy

- Keep GridCraft's crates unmodified where possible; put project code in new crates.
- Merge `storytold/gridcraft` updates by hand when wanted (`upstream` remote).
- Changes needed in shared crates are kept small and noted here so merges stay easy.

**Phase 0 changes to keep on upstream merges** (2026-11-01):

- `README.md` is PlanCraft's (GridCraft's is at `docs/gridcraft-README.md`): keep ours.
- Workspace version is PlanCraft's (`0.1.0` onwards) and `repository` points here: keep ours.
- Apps renamed: `apps/plancraft`, `apps/plancraft-cli` (packages `plancraft`, `plancraft-cli`);
  `xtask/src/layers.rs` also maps `plancraft-*` crates.
- `crates/ui-egui/src/lib.rs`: `APP_NAME`, `ACCENT` and `UPDATE_APP` (updates come from this
  repo's releases); message-box titles use `APP_NAME`; About and File › Account show PlanCraft.
- Packaging renamed (`packaging/windows/plancraft.wxs` with its own UpgradeCode, Linux/macOS
  files), new icon (`assets/app-icon/plancraft.*`).
- Upstream-only workflows removed (release, SonarCloud, FreeBSD, Nix, Windows arm64) and
  `omastore.toml`, `sonar-project.properties`, `.coderabbit.yaml`; PlanCraft's Windows build is
  `.github/workflows/plancraft-windows.yml`.
- Not yet renamed (later): translated UI strings that say GridCraft, MCP server name, the
  control-channel token folder, `GRIDCRAFT_*` environment variables.

## Open decisions (for planning before work starts)

- [x] App name: PlanCraft; repo Azurehawke/plancraft (decided 2026-11-01)
- [x] GridCraft comes in with its full git history, `storytold/gridcraft` as `upstream` (decided 2026-11-01)
- [x] Sync library: Loro, with Automerge as fallback if the phase 1 prototype test fails (decided 2026-10-31)
- [x] Project on disk: `Name.plan\` folder with `Name.plancraft` entry file inside (decided 2026-11-01)
- [ ] Which MS Project features matter for the first usable version (scope of phase 3)
- [ ] Desktop-first, or Android in parallel from the start?
- [x] RevuCraft: optional link in Settings; PlanCraft stays standalone; punch items become tasks automatically (decided 2026-11-01)
- [ ] RevuCraft link transport: `markups.json` sidecar or RevuCraft as a Loro "device"; plus the open questions in `docs/revucraft-integration.md`
- [x] PlanCraft is the hub of the Craft suite; RevuCraft and others are spokes (decided 2026-11-05)
- [x] Documents and templates are the most immediate suite item; work waits until Steve starts it (decided 2026-11-05)
- [ ] Documents: Markdown templates first, or .docx through WordCraft? (proposed 2026-11-05: Markdown first)
- [ ] Originality rule for PMBOK-style templates: our own outline and wording (proposed 2026-11-05)
- [ ] Finance: does the budget and ledger module live in PlanCraft or CraftStudio?
- [ ] Forms and signatures: which legal requirements apply (jurisdictions, consent, audit trail)
- [ ] Team messages: file-based first; is a relay server ever needed?
- [ ] RevuCraft two-way exchange: message format, field ownership, stable IDs (after the one-way link is confirmed to keep working)
- [ ] Costs / financial tracking scope (CraftStudio's planned financial module)
- [ ] Standalone app vs future CraftStudio module
- [x] Interface layout: Obsidian-style shell (decided 2026-10-27)
- [x] Left sidebar with switchable panels like Obsidian: Explorer, Search, Bookmarks (decided 2026-10-27)
- [x] Project-specific panels: Outline/WBS, Filters / saved views, Recent, Sync; right sidebar Inspector (decided 2026-10-27)
- [ ] Explorer details: what it shows beyond files (e.g. virtual items like Backlog, Sprints), and default split layouts
- [ ] Final template list, and which templates ship in the first version
- [ ] Template field syntax (WordCraft merge fields vs content controls) and how repeating tables are marked
- [ ] Which document tables import back into the project, and their required headings
- [ ] Agile model scope (story points, velocity, Kanban WIP limits) and how hybrid projects show on the Gantt
- [ ] Lock-note behaviour for documents (warn only, or read-only on other devices)
- [ ] Project mail: how it looks and works; project mail only, or a fuller client (inbox, folders, calendar)?
- [x] Obsidian plugin as a later phase, with the four design rules applied from phase 1 (decided 2026-10-29)
- [ ] Obsidian plugin: Markdown templates, and which views it offers first
- [x] Optional new project wizard (decided 2026-10-29)
- [ ] Wizard: final field list and step order
- [ ] Mail accounts to support first (IMAP/SMTP, Gmail, Microsoft 365) and how sign-in works per device
