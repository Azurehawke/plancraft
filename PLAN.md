# PlanCraft — plan

> **Status:** planning only. No code yet. Working name "PlanCraft" until a final name is chosen.
> **Repo:** `Azurehawke/plancraft` (created empty; holds only this plan until work starts).
> **Base:** GridCraft, to be imported from [`Azurehawke/gridcraft`](https://github.com/Azurehawke/gridcraft) (upstream [`storytold/gridcraft`](https://github.com/storytold/gridcraft), MIT OR Apache-2.0).
> **Last updated:** 2026-10-26

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

## Reference projects

GridCraft's rules forbid copying GPL/AGPL/LGPL code, and PlanCraft keeps GridCraft's
MIT/Apache licence. These are **behaviour references** (study, observe, test against), not code sources:

| Project | Licence | What to learn |
|---|---|---|
| ProjectLibre | CPAL | Closest MS Project clone: ribbon, WBS/RBS charts, earned value, MS Project import/export |
| GanttProject (`bardsoftware/ganttproject`) | GPL | Gantt interaction, baselines, resource charts |
| TaskJuggler (`taskjuggler/TaskJuggler`) | GPL | Scheduling and resource leveling |
| MPXJ (`joniles/mpxj`) | LGPL-2.1 | MS Project field mapping; reads .mpp and ~20 other formats, writes MSPDI. Use as a test oracle / external converter only |

**Code that can be used directly** (permissive): Automerge or Loro (sync), `petgraph` (if a
separate task graph is wanted), plus GridCraft itself.

## Phases

| # | Phase | Done when |
|---|---|---|
| 0 | Repo setup: import GridCraft's history into this repo, add `upstream`, rename app/binaries, keep shared crates untouched | Builds and runs as PlanCraft on Windows |
| 1 | `project-model` + `project-sync` + folder format | Same project edited offline on laptop and tablet, synced by Syncthing, merges cleanly |
| 2 | `schedule` crate (CPM, calendars, constraints) | Fixture suite passes |
| 3 | Task sheet + Gantt view + `task.*` commands | Usable for a real network-install job |
| 4 | Resources, assignments, usage views | Resource over-allocation visible |
| 5 | Baselines and tracking | Tracking Gantt shows slip against baseline |
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
