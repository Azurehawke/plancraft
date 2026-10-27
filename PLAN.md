# PlanCraft — plan

> **Status:** planning only. No code yet. Working name "PlanCraft" until a final name is chosen.
> **Repo:** `Azurehawke/plancraft` (created empty; holds only this plan until work starts).
> **Base:** GridCraft, to be imported from [`Azurehawke/gridcraft`](https://github.com/Azurehawke/gridcraft) (upstream [`storytold/gridcraft`](https://github.com/storytold/gridcraft), MIT OR Apache-2.0).
> **Last updated:** 2026-10-27

## Goal

A local-first, MS Project-style project management app in Rust, built on GridCraft's engine and UI.
It runs locally on every device (Windows laptop, Surface Pro 7s, Galaxy Tab, Pixel phone) and
saves projects into a shared folder synced by Syncthing. The same project can be edited on several
devices, including offline, and the edits merge cleanly.

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
| `project-model` | L2 | Tasks, WBS outline, dependencies, calendars, resources, assignments, baselines, custom fields |
| `schedule` | L2 | Critical path scheduler: forward/backward pass, slack, constraints, summary rollups; later resource leveling |
| `project-sync` | L2 | CRDT document (Automerge or Loro), per-device change files, merge on open, file watching |
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

- **Left: project explorer.** Shows the project folder as it is on disk: schedule, backlog,
  board, `docs\`, attachments. Collapses to an icon rail (toggle) to give the Gantt full width.
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

### Views inside a schedule tab

A schedule tab has a small view switcher in its corner (Affinity-style: same data, different
toolbar): Gantt, Tracking Gantt, Network Diagram, Resource Sheet, Task Usage, Resource Usage,
Calendar, Kanban.

### Phone and tablet

- Explorer becomes a flyout (as in RevuCraft's phone layout); tabs become a switcher.
- Same layout scales from the Pixel to the Galaxy Tab and desktop.

### CraftStudio path

The same shell could later hold drawings, schedules and documents as different tab types, which
is close to the mode switching planned for CraftStudio.

## Multi-device design

**Problem.** If two devices edit one file and Syncthing syncs it, Syncthing keeps one and writes
`.sync-conflict` copies, so one device's work is effectively lost.

**Approach.** Each project is a folder; each device writes only its own file:

```
M:\Jobs\Acme-Network\Acme-Network.plan\
    project.json             # project id, name, format version
    devices\
        laptop.am            # this device's CRDT changes (only the laptop writes it)
        surface-1.am
        galaxy-tab.am
    docs\                    # .docx documents made from templates (+ lock notes)
    attachments\             # photos, linked PDFs (optional)
```

- On open: load every file in `devices\`, merge into one CRDT document, build the model.
- On save: append this device's new changes to its own file only.
- While open: watch the folder; when Syncthing delivers another device's file, merge it live.
- Conflicting edits to the *same* field resolve deterministically (CRDT rules); edits to
  different fields or tasks always merge.
- Occasionally compact: a device may write a merged snapshot, after which old change files can be trimmed.

**Libraries to evaluate:** Automerge (MIT, Rust) and Loro (MIT, Rust). Decide by: file size,
merge speed on a few thousand tasks, Android build, and how history is kept.

The same pattern could later be backported to RevuCraft if it uses one shared file.

## Documents and templates

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

## Reference projects

GridCraft's rules forbid copying GPL/AGPL/LGPL code, and PlanCraft keeps GridCraft's
MIT/Apache licence. These are **behaviour references** (study, observe, test against), not code sources:

| Project | Licence | What to learn |
|---|---|---|
| ProjectLibre | CPAL | Closest MS Project clone: ribbon, WBS/RBS charts, earned value, MS Project import/export |
| GanttProject (`bardsoftware/ganttproject`) | GPL | Gantt interaction, baselines, resource charts |
| TaskJuggler (`taskjuggler/TaskJuggler`) | GPL | Scheduling and resource leveling |
| MPXJ (`joniles/mpxj`) | LGPL-2.1 | MS Project field mapping; reads .mpp and ~20 other formats, writes MSPDI. Use as a test oracle / external converter only |

More reference repos to be added after further research.

**Code that can be used directly** (permissive): Automerge or Loro (sync), `petgraph` (if a
separate task graph is wanted), plus GridCraft itself.

## Phases

| # | Phase | Done when |
|---|---|---|
| 0 | Repo setup: import GridCraft's history into this repo, add `upstream`, rename app/binaries, keep shared crates untouched | Builds and runs as PlanCraft on Windows |
| 1 | `project-model` + `project-sync` + folder format | Same project edited offline on laptop and tablet, synced by Syncthing, merges cleanly |
| 2 | `schedule` crate (CPM, calendars, constraints) | Fixture suite passes |
| 3 | Obsidian-style shell (explorer, tabs, per-tab ribbon), task sheet + Gantt view + `task.*` commands | Usable for a real network-install job |
| 4 | Resources, assignments, usage views | Resource over-allocation visible |
| 5 | Baselines and tracking | Tracking Gantt shows slip against baseline |
| 5b | Agile model (backlog, sprints, board, burndown) and document templates (fill + import back) | A sprint planned in PlanCraft, its planning doc typed in WordCraft, stories imported back |
| 6 | MSPDI import/export; XLSX and PDF reports | A project round-trips through MS Project XML |
| 7 | Resource leveling, Network Diagram | — |
| 8 | Android build (Craft port conventions: self-update from repo, phone layout) | Runs on the Galaxy Tab and Pixel |
| 9 | Optional: module inside CraftStudio | — |

## Upstream policy

- Keep GridCraft's crates unmodified where possible; put project code in new crates.
- Merge `storytold/gridcraft` updates by hand when wanted.
- Changes needed in shared crates are kept small and noted here so merges stay easy.

## Open decisions (for planning before work starts)

- [ ] Final app name and repo name
- [ ] How to bring GridCraft in: import its full history (keeps upstream merges easy) or vendor its crates?
- [ ] Automerge vs Loro
- [ ] Project folder extension and layout
- [ ] Which MS Project features matter for the first usable version (scope of phase 3)
- [ ] Desktop-first, or Android in parallel from the start?
- [ ] Integration with RevuCraft (link tasks to drawing sheets, field photos, site notes)?
- [ ] Costs / financial tracking scope (CraftStudio's planned financial module)
- [ ] Standalone app vs future CraftStudio module
- [x] Interface layout: Obsidian-style shell (decided 2026-10-27)
- [ ] Explorer details: what it shows beyond files (e.g. virtual items like Backlog, Sprints), and default split layouts
- [ ] Final template list, and which templates ship in the first version
- [ ] Template field syntax (WordCraft merge fields vs content controls) and how repeating tables are marked
- [ ] Which document tables import back into the project, and their required headings
- [ ] Agile model scope (story points, velocity, Kanban WIP limits) and how hybrid projects show on the Gantt
- [ ] Lock-note behaviour for documents (warn only, or read-only on other devices)
