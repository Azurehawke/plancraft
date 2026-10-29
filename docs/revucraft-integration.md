# PlanCraft and RevuCraft: integration notes

> **Status:** proposal, for planning. Nothing here is built on the PlanCraft side yet.
> **Written by:** the RevuCraft session (repo `Azurehawke/pdfcraft`, branch `revucraft`), for
> the PlanCraft planning session. Reply by editing this file (add a dated note under
> [Decisions and replies](#decisions-and-replies)) rather than `PLAN.md`, so the two sessions
> never edit the same file.
> **Last updated:** 2026-10-28 (RevuCraft build 207; live dashboard added)
>
> This answers the open decision in `PLAN.md`: "Integration with RevuCraft (link tasks to
> drawing sheets, field photos, site notes)?"

## Goal

Steve runs a job in both apps: the drawings, markups, field notes, photos and punch list in
RevuCraft; the schedule, backlog and project documents in PlanCraft. Each app should be able to
point at the other's items, and progress recorded in the field should reach the schedule without
retyping.

## What RevuCraft already has (as of build 207)

| Feature | How it is stored | Useful to PlanCraft as |
|---|---|---|
| **Job folder** beside each PDF: `Smith Residence.pdf` → `Smith Residence/` | A folder of real files; paths stored relative to the PDF | The place a job's drawings and notes live |
| **Markdown notes** pinned on a sheet | `Smith Residence/notes/Note 1.md` (the master copy); the PDF keeps a copy of the text | Site notes, readable by Obsidian and by PlanCraft |
| Notes' **checklists**, `#tags`, `[[links]]` to markups | Plain Markdown in the `.md` file | Task-like items and progress ("3 of 5 done") |
| **Punch list** | On each markup in the PDF: `/PCPunch << /No /Assignee /Due /Status >>`; status Open, In progress, Ready for review, Closed | Field issues with owner, due date and status |
| **Spaces** (named rooms or zones) | A polygon or rectangle markup with `/PCSpace true`, its name in `/Contents` | Locations or work areas |
| **Photo pins** | The photo is inside the PDF (`/PCPhoto`) | Evidence for a task or status report |
| **Links box** | Drawings (DWG/DXF) and workbooks (.xlsx) linked by relative path, with "Modified" when the file changed | The existing pattern for linking files |
| **Markups list** | Every markup with page, type, label, layer, author, status, space, custom columns; CSV export | A table PlanCraft could import |
| **File watching + Reload from disk** | Reloads a PDF when Syncthing replaces it | Already works in a Syncthing-shared jobs folder |

Every RevuCraft markup has a PDF annotation name (`/NM`) that stays the same when other markups
are added or removed (its position in the page's list does not). That name, plus the PDF's
relative path, is a stable address for a markup from outside the PDF.

## Proposal

### 1. One job folder for both apps

Agree on one layout so a job's files sit together and Syncthing syncs them as one folder:

```
M:\Jobs\Acme-Network\
    Acme-Network.plan\          # PlanCraft project (PLAN.md's folder format)
    Drawings\
        A-201 Floor plan.pdf    # RevuCraft drawing
        A-201 Floor plan\       # its job folder (RevuCraft)
            notes\Note 1.md
    Budget.xlsx
```

Open question: should RevuCraft's per-PDF folder move under a job-wide folder (e.g.
`Acme-Network\notes\A-201\`) when a `.plan` project sits beside it? Today RevuCraft has no notion
of a job wider than one PDF.

### 2. Links in both directions

A link is text, so either app can store it and the other can follow it:

| From | To | Link form (proposal) |
|---|---|---|
| PlanCraft task | A sheet | `Drawings/A-201 Floor plan.pdf#page=3` (relative to the job folder) |
| PlanCraft task | A markup, punch item or space | `Drawings/A-201 Floor plan.pdf#nm=<annotation name>` |
| PlanCraft task | A note | `Drawings/A-201 Floor plan/notes/Note 1.md` |
| RevuCraft markup or note | A task | `Acme-Network.plan#task=<task id>` |

- **Opening a link from PlanCraft**: hand the path to RevuCraft (as a file open with a
  `--goto page|nm` argument; RevuCraft would add that argument).
- **Opening a link from RevuCraft**: a task link in a note (`[[task:1.2.3]]`, or a Markdown link)
  or a markup's Link field opens PlanCraft at the task.
- Stable task ids are needed for this (WBS codes change when tasks move); `project-model`'s task
  id would be the target, with the WBS code shown as the label.

### 3. Progress from the field

- **Punch items → tasks.** A punch item linked to a task (or a task created from a punch item)
  takes its status: Closed counts as done; % complete of a summary task could roll up from its
  linked punch items.
- **Checklists → % complete.** A note's "3 of 5 done" can set a linked task's % complete.
- **Spaces → work areas.** A task can name a space ("Pull cable: Server room"); the Gantt or a
  filter can group by space, and RevuCraft can show which spaces have open tasks.
- **Photos and notes → status reports.** The status-report template could list the photos and
  notes linked to the tasks in the reporting period.

How PlanCraft reads RevuCraft's data, options:

1. Read the PDFs directly (punch items and spaces are plain annotation entries; the Markdown
   notes are plain files). Needs a small PDF reader in PlanCraft.
2. RevuCraft writes a sidecar file in the job folder (e.g. `A-201 Floor plan/markups.json`) every
   save: markups with their names, pages, spaces, punch data and links. PlanCraft reads only that.
   Simplest for PlanCraft, and readable by other tools.
3. Both apps share a small Rust crate (in craft-libs or a new repo) that reads and writes the
   link format and the sidecar.

The RevuCraft session's recommendation: **option 2, with the link format in a shared crate later
(option 3)**. The sidecar is one function on the RevuCraft side and keeps PlanCraft free of PDF
parsing.

### 4. Live dashboard (Steve's request, 2026-10-28)

Steve wants:

- Ticking off a punch item or task anywhere is reflected in the `.plan` and everywhere else that
  shows it.
- PlanCraft produces **output that updates as the plan changes**, probably HTML laid out for
  letter-size paper.
- That output has a **summary sheet** that can be **placed on a sheet of a RevuCraft PDF**, and is
  interactive there too: completing a punch item updates the placed dashboard.
- The whole module is **optional per PDF**.

**Constraint.** A PDF page cannot hold a live web page, and RevuCraft has no web view (Rust and
egui only). So the placed summary is not the HTML itself: RevuCraft draws the same content
natively, as it already draws a live, placed view of a linked workbook (Place table, Workbook
box).

**Design.**

1. **One dashboard description, two renderers.** A small, versioned description of the summary
   (sections, figures, tables, which items each row stands for, page size) is produced from the
   plan and the field data. PlanCraft renders it to the letter-size HTML report; RevuCraft
   renders it as a placed panel on a sheet. Both read the same description, so they never drift.
   The description and its builder are the natural first piece of a shared crate (option 3 in
   [Progress from the field](#3-progress-from-the-field)).
2. **Optional per PDF.** A PDF opts in by linking a `.plan` in RevuCraft's Links box (as with
   drawings and workbooks), then placing the summary on a sheet. Unlinked PDFs carry nothing extra.
3. **Live.** RevuCraft already watches linked files: when the `.plan` changes (PlanCraft saved, or
   Syncthing delivered another device's change file), the panel redraws within seconds.
4. **Interactive in RevuCraft.** Tapping a row goes to its task (in PlanCraft) or punch item (on
   the sheet). Ticking a punch item, on the sheet or in the panel, updates the panel, RevuCraft's
   Punch list and Markups list, and (through `markups.json`, below) the plan.
5. **Sent or printed.** The PDF keeps the panel's last drawn appearance, stamped "as of
   <date and time>", so it prints and reads correctly without the job folder.

**Who writes what (recommended).** Each app writes only its own data:

- RevuCraft owns punch items: in the PDF, mirrored to `<drawing>/markups.json` on every save.
- PlanCraft owns tasks: it reads `markups.json`, and a task linked to punch items takes their
  status (all Closed = done; % complete rolls up from them).
- Nothing writes into the other app's files, so the per-device CRDT files are never touched by
  RevuCraft and Syncthing never sees two writers on one file.

Alternative, later: RevuCraft writes its own device change file into the `.plan`'s `devices\`
folder (it becomes another "device"). More direct, but it must wait until Automerge or Loro is
chosen, and it puts PlanCraft's format inside RevuCraft.

### 5. Sync

PlanCraft's "each device writes its own change file" design is also what RevuCraft would need
for several devices editing one drawing. If PlanCraft settles on Automerge or Loro, RevuCraft
could later store its markups the same way (PLAN.md already mentions backporting it). Until then
RevuCraft relies on one device editing a PDF at a time, plus file watching and Reload from disk.

## Open questions

- [ ] Job folder layout: per-PDF folders (today) or one job folder with a `.plan` beside the drawings?
- [ ] Link format: `#page=` / `#nm=` / `#task=` as above, or something else?
- [ ] How PlanCraft reads RevuCraft data: sidecar file (recommended), direct PDF reading, or a shared crate?
- [ ] Which progress signals drive % complete: punch status, checklists, both, or manual only?
- [ ] Does a punch item create a task, or only link to one?
- [ ] Spaces as a PlanCraft field (location) on tasks?
- [ ] Dashboard description: what goes on the summary sheet (milestones, % complete, critical
      path, open punch items by space or assignee, overdue items, recent photos)?
- [ ] Dashboard description format and owner (shared crate in craft-libs or a new repo; JSON
      or a Rust type serialised with serde)?
- [ ] HTML output: one file per report regenerated on save, or served live by PlanCraft?
- [ ] Ticking a *task* (not a punch item) in RevuCraft's panel: allowed, and if so how does it
      reach the plan without RevuCraft writing the `.plan` (a request file PlanCraft applies, or
      the "RevuCraft as a device" alternative)?
- [ ] Should CraftStudio's modes (PLAN.md "CraftStudio path") replace this cross-app linking later?

## Decisions and replies

(Add dated notes here: `- 2026-10-29 (PlanCraft): …`)

- 2026-10-28 (Steve, via the RevuCraft session): **What the summary sheet holds.** Steve will make
  a mock-up. As he sees it now:
  - Project title and a short summary of its purpose
  - Start and end dates
  - Project manager, with contact information
  - Participants, their roles and the tasks assigned to each
  - A snapshot of the Gantt chart, tasks coloured by assignee
  - Trackers: budget, a countdown to the deadline, and a few others

  Data-model consequences for PlanCraft (not yet in `PLAN.md`'s draft): a project manager and
  contact details (phone, email) on resources or a project team list; a role per participant; a
  colour per resource for the Gantt; a project budget against which cost is tracked (ties to the
  open "costs / financial tracking scope" decision); a deadline the countdown reads (a project
  deadline, or the finish of a chosen milestone). The Gantt snapshot in RevuCraft's panel is drawn
  from the dashboard description (bars, dates, colours), not as a picture, so it stays sharp and
  live.
- 2026-10-28 (Steve, via the RevuCraft session): **HTML output lives in its own file**, rewritten
  when the plan changes, the way RevuCraft watches linked DWG/DXF files. Proposed shape: PlanCraft
  writes two files in the project folder, e.g. `reports/summary.html` (the letter-size report, for
  people) and `reports/summary.json` (the dashboard description, for RevuCraft); RevuCraft links and
  watches the `.json`, never parses the HTML. Rewritten whenever the merged plan changes. Because
  several devices may each regenerate them, the content must be deterministic from the merged plan
  (no device names or wall-clock times inside, except a "plan as of" stamp taken from the plan's
  own last change), written atomically (temp file + rename), and skipped when unchanged, so
  Syncthing sees identical files rather than conflicts.
