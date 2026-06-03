# ghs — GitHub Actions Status

An interactive, refined terminal dashboard that shows the **GitHub Actions
status** of all your configured repositories at a glance — built in Rust with
[ratatui](https://ratatui.rs).

```
  ⬡ GitHub Actions Status  ·  2 projects
╭ Projects ──────────────────╮╭ ratatui/ratatui ───────────────────╮
│ ▌  ✓ Ratatui      Success  ││  Recent runs   updated just now     │
│    ✓ rust-lang/rust …      ││  ✓ Success  Continuous Integration  │
╰────────────────────────────╯╰─────────────────────────────────────╯
  ↑↓ navigate  ·  r refresh  ·  o open  ·  ? help  ·  q quit
```

## Features

- **Live master/detail TUI** — project list with attention-first sorting
  (failures float to the top), then ordered by most-recent CI activity.
- **Search** (`/`) — incrementally filter projects as you type.
- **Filter by owner/company** (`f`) — instantly show only one org's repos.
- **Bounded-concurrent fetching** — projects load in parallel (capped by a
  semaphore so hundreds of repos won't trip rate limits); the list updates
  incrementally as results arrive.
- **Auto-refresh** on a configurable timer, plus manual `r` refresh.
- **Open in browser** (`o` / Enter) jumps to the latest run on github.com.
- **Headless modes** for scripting: `ghs list` and `ghs check` (CI-friendly,
  non-zero exit on failures).
- **Refined UI/UX** — a calm "midnight" palette, semantic status colors,
  animated spinners, transient toasts for feedback, filter chips, a
  mode-aware footer, and a help overlay (`?`).

## Install

```sh
./install.sh             # builds + installs `ghs` to your PATH, then a setup wizard
# alternatives:
cargo install --path .
cargo run --release -- --help
```

The installer detects a writable dir on your `PATH`, installs the binary, runs
`ghs doctor`, and offers to populate your config automatically. Pass
`--no-setup` to skip the wizard, or `--uninstall` to remove.

## Quick start (zero config)

Auth is **auto-detected** — if you already use the [GitHub CLI](https://cli.github.com)
(`gh auth login`) or have `GITHUB_TOKEN` set, there's nothing to configure.

```sh
ghs import --me          # pull in your GitHub repos…
# …or…
ghs import --local       # …discover repos you've already cloned
# …or…
ghs add rust-lang/rust ratatui/ratatui   # add specific ones

ghs                      # launch the dashboard
```

No config file? No problem — `ghs` starts with a friendly empty state that
shows you exactly how to add repositories. Run `ghs doctor` anytime to check
your setup.

## Adding repositories

You never have to hand-edit TOML (though you can):

| Command                          | What it does                                  |
|----------------------------------|-----------------------------------------------|
| `ghs add owner/repo`             | Track a repo (accepts `@branch` and URLs)     |
| `ghs add a/b c/d@main`           | Add several at once                           |
| `ghs remove owner/repo`          | Stop tracking a single repo                   |
| `ghs remove owner`               | Stop tracking a **whole owner/company**       |
| `ghs import --local [DIR]`       | Scan a folder for git clones with GitHub remotes |
| `ghs import --me`                | Import your own GitHub repositories           |
| `ghs import --user NAME`         | Import a user's public repos                  |
| `ghs import --org NAME`          | Import an organization's repos                |
| `ghs import … --dry-run`         | Preview without writing                       |

All of these create the config on demand and de-duplicate automatically.

You can also manage repositories **interactively inside the TUI** — press `d`
to remove the selected repo, or `D` to remove every repo of its owner; a
confirm prompt protects against mistakes and the change is saved to your
config immediately. (Removing only affects your dashboard, never the repo on
GitHub.)

## Configuration

Config lives at the platform config dir (`ghs where` prints the path), e.g.
`~/.config/github-status/config.toml`:

```toml
[settings]
refresh_interval_secs = 60   # 0 disables auto-refresh
runs_per_project = 5
max_concurrency = 8          # cap on simultaneous API requests per refresh
# token = "ghp_..."          # prefer the GITHUB_TOKEN env var instead

[[project]]
owner = "ratatui"
repo  = "ratatui"

[[project]]
owner  = "rust-lang"
repo   = "rust"
branch = "master"            # optional branch filter
label  = "Rust"             # optional display name
```

**Token resolution order (auto):**
`GITHUB_TOKEN` → `GH_TOKEN` → `settings.token` → `gh auth token`.
Most users never set anything.

## Commands

| Command       | Description                                              |
|---------------|---------------------------------------------------------|
| `ghs`         | Launch the interactive TUI (default)                    |
| `ghs list`    | One status line per project, then exit                  |
| `ghs check`   | Exit non-zero if any project's latest run is failing    |
| `ghs add`     | Add repositories                                        |
| `ghs remove`  | Remove repositories                                     |
| `ghs import`  | Bulk-import from local clones or GitHub                  |
| `ghs doctor`  | Diagnose token, `gh`, config, and connectivity          |
| `ghs init`    | Scaffold a config file (`--force` to overwrite)         |
| `ghs where`   | Print the resolved config path                          |

`--config <FILE>` overrides the config location for any command.

## Keys

| Key                | Action                                  |
|--------------------|-----------------------------------------|
| `↑`/`k`, `↓`/`j`   | Navigate                                |
| `g` / `G`          | Jump to top / bottom                    |
| `r`                | Refresh all projects                    |
| `/`                | Search (type to filter, Enter applies)  |
| `f`                | Show only the selected owner/company    |
| `c`                | Clear search & filters                  |
| `d`                | Remove the selected repo (asks to confirm) |
| `D`                | Remove ALL repos of the selected owner  |
| `o` / `Enter`      | Open latest run in browser              |
| `?`                | Toggle help                             |
| `Esc`              | Cancel prompt → exit search → clear filter → quit |
| `q` / `^C`         | Quit                                    |

## Architecture

Hexagonal (Ports & Adapters). Dependencies point **inward**; the pure
`domain` core never depends on octocrab or ratatui.

```
  tui ─┐                       (presentation: ratatui)
       ├─▶ app ─▶ ports ◀─ adapters   (orchestration ▶ abstraction ◀ impls)
  cli ─┘            ▲
                 domain                (pure types — the stable core)
```

| Module       | Responsibility                                                      |
|--------------|---------------------------------------------------------------------|
| `domain`     | Pure types & rules. SSoT for status semantics (`RunState`); `Project::parse`. |
| `ports`      | The `StatusProvider`, `RepoDiscovery` and `ProjectStore` traits the app depends on. |
| `adapters`   | octocrab provider (status + discovery), a `LocalGitScanner` reading `.git/config`, and a `FileProjectStore` persisting the project list. |
| `app`        | UI-agnostic state machine (`AppState`), actions, orchestration.     |
| `tui`        | ratatui widgets + async runtime. `theme` is the SSoT for the palette. |
| `cli`/`commands` | clap args + headless entry points (status / manage / import / doctor). |
| `config`     | TOML load/save, project add/remove, multi-source token resolution.  |

The **`import`** feature shows the architecture paying off: a second port
(`RepoDiscovery`) with two adapters (GitHub API and a dependency-free local
`.git/config` scanner) plugs into the same composition root — no changes to the
domain, app, or TUI.

`main.rs` is the composition root — it wires the concrete adapter into the
app behind the port. Swapping the backend (GraphQL, a cache, a mock) means
writing one new adapter; nothing else changes.

### Why these choices

- **`RunState` as SSoT:** GitHub encodes outcomes across two raw strings
  (`status` + `conclusion`). They're collapsed into one exhaustive enum in
  exactly one place (`domain::status`), so no status logic is duplicated.
- **Elm-style reducer:** all state changes flow through `AppState::update`,
  which returns a `Command` describing side effects. State stays pure and
  unit-testable; the runtime is a thin side-effecting shell.
- **Ports & Adapters:** the app and TUI depend only on the `StatusProvider`
  trait, never octocrab — proven by the headless commands reusing the same
  core, and by the mock-provider tests.
- **Derived, not duplicated:** the filtered/searched list is computed on demand
  from `projects` (`AppState::visible_indices`) — never stored as a second list
  that could drift. `selected` indexes the *visible* list and stays put across
  re-sorts (the cursor doesn't chase a row as failures float up).
- **Ordering as a domain rule:** `ProjectStatus::cmp_display` is the single,
  allocation-free comparator — attention bucket (failures first), then most
  recent CI activity, then owner/repo. Cheap enough to run on every refresh.
- **Bounded concurrency:** `StatusService` gates fetches behind a `Semaphore`
  so tracking hundreds of repos can't spawn hundreds of simultaneous requests.

## Development

```sh
cargo test            # unit tests (domain, config, state reducer, keymap)
cargo clippy --all-targets
cargo run -- list -c ./my-config.toml
```

Logs are written to the platform data dir (`…/logs/ghs.log`); set `GHS_LOG`
(e.g. `GHS_LOG=debug`) to adjust verbosity. Logging never touches stdout, so
it can't corrupt the TUI.

## License

MIT
