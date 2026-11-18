# Releases and the installer

nestpro's Windows installer and updates use [Velopack](https://velopack.io): the `velopack`
Rust crate inside the program, and its `vpk` command-line tool to pack a release. This page
records what their public documentation says about the parts the program relies on (spec
#178, ADR 0006), with a link to each source, and then how this repo makes a release.

Versions read: crate [`velopack` 1.2.161](https://docs.rs/velopack/1.2.161/velopack/) and the
`vpk` tool of the same version (both pinned in `app/Cargo.toml` and
`.github/workflows/release.yml`).

## What Velopack does

### 1. The start-up hook the program must call first

- The program calls `VelopackApp::build().run()` as the first thing in `main`. The crate says it
  "should be the first thing to run in your app" and "in some circumstances it may
  terminate/restart the process to perform tasks."
  ([VelopackApp](https://docs.rs/velopack/1.2.161/velopack/struct.VelopackApp.html),
  [Rust quick start](https://docs.velopack.io/getting-started/rust))
- While installing, updating and uninstalling, the installer starts the program with its own
  arguments (`--veloapp-install`, `--veloapp-updated`, `--veloapp-obsolete`,
  `--veloapp-uninstall`, then a version). `run()` handles them and exits the process, so the
  program must not open a window or do any work before it. Hooks registered for these run
  first, show no window, and are stopped after 30 s (install, uninstall) or 15 s (update).
  ([Integrating overview](https://docs.velopack.io/integrating/overview),
  [crate source, app.rs](https://docs.rs/velopack/1.2.161/src/velopack/app.rs.html),
  [constants.rs](https://docs.rs/velopack/1.2.161/src/velopack/constants.rs.html))
- A copy that the installer didn't install (a developer build, a copied `.exe`) finds no
  install records; `run()` logs that and returns, and the program carries on.
  ([app.rs](https://docs.rs/velopack/1.2.161/src/velopack/app.rs.html))
- By default `run()` also applies an update that was downloaded earlier, at start-up
  ("ON by default"). `set_auto_apply_on_startup(false)` turns that off, so updates are applied
  only where the program asks for it.
  ([VelopackApp::set_auto_apply_on_startup](https://docs.rs/velopack/1.2.161/velopack/struct.VelopackApp.html#method.set_auto_apply_on_startup),
  [Integrating overview](https://docs.velopack.io/integrating/overview))
- `on_first_run` and `on_restarted` register code for the first start after installing and the
  start after an update.
  ([VelopackApp](https://docs.rs/velopack/1.2.161/velopack/struct.VelopackApp.html))

### 2. A folder as the update source

- `UpdateManager::new(source, options, locator)` takes the place updates come from. It returns
  an error when the program isn't installed.
  ([UpdateManager](https://docs.rs/velopack/1.2.161/velopack/struct.UpdateManager.html))
- `sources::FileSource::new(path)` "retrieves available updates from a local or
  network-attached disk." The folder must hold the packages and a `releases.{channel}.json`
  index; on Windows the channel is `win` unless the release was packed with another.
  ([FileSource](https://docs.rs/velopack/1.2.161/velopack/sources/struct.FileSource.html),
  [Update sources](https://docs.velopack.io/integrating/update-sources),
  [Packaging overview](https://docs.velopack.io/packaging/overview))
- The files `vpk pack` writes are the index (`releases.{channel}.json`), the full package
  (`<id>-<version>-full.nupkg`), a delta package when an earlier release was in the output
  folder, the installer (`Setup.exe`) and a portable zip. Copying the index and the packages
  into a folder makes it an update source.
  ([Packaging overview](https://docs.velopack.io/packaging/overview))
- The documentation doesn't say whether a Windows network path (`\\server\share`) works as the
  folder. That is checked on real PCs, not assumed.
  ([Update sources](https://docs.velopack.io/integrating/update-sources))
- For a PC of its own, `sources::GithubSource::new(repo_url, token, prerelease)` reads the
  releases of a GitHub repository; without a token it is limited to 60 requests an hour per
  address.
  ([sources](https://docs.rs/velopack/1.2.161/velopack/sources/index.html),
  [Update sources](https://docs.velopack.io/integrating/update-sources))

### 3. Installing an exact or an older version

- `check_for_updates()` offers the newest release in the source. With
  `UpdateOptions { AllowVersionDowngrade: true, .. }` it also offers it when that release is
  older than the installed one, but it always picks the newest in the source, not a chosen one.
  ([UpdateOptions](https://docs.rs/velopack/1.2.161/velopack/struct.UpdateOptions.html),
  [Installing a specific version](https://docs.velopack.io/integrating/specific-version))
- An exact version: read the source's list with `get_release_feed()`, pick the full package of
  that version, and build an `UpdateInfo` for it (`TargetFullRelease`, with `IsDowngrade: true`
  when it is older than the installed one), then download and apply it as usual.
  ([UpdateManager::get_release_feed](https://docs.rs/velopack/1.2.161/velopack/struct.UpdateManager.html#method.get_release_feed),
  [UpdateInfo](https://docs.rs/velopack/1.2.161/velopack/struct.UpdateInfo.html),
  [Installing a specific version](https://docs.velopack.io/integrating/specific-version))
- Going back uses the full package only (no deltas), deletes the newer packages kept on the
  PC so it doesn't update itself straight back, and must be applied explicitly: start-up
  auto-apply only installs newer versions. The version must still be in the source.
  ([UpdateInfo](https://docs.rs/velopack/1.2.161/velopack/struct.UpdateInfo.html),
  [Installing a specific version](https://docs.velopack.io/integrating/specific-version))

### 4. Applying an update when the program exits

- `download_updates(&info, progress)` fetches the release into the PC's packages folder; it
  tries delta packages first. `get_update_pending_restart()` says whether a downloaded update
  is waiting.
  ([UpdateManager](https://docs.rs/velopack/1.2.161/velopack/struct.UpdateManager.html))
- `wait_exit_then_apply_updates(asset, silent, restart, args)` starts the updater, which waits
  for this program to exit, then applies the update and, if asked, starts the program again.
  The program should finish and exit right after calling it: the updater waits 60 seconds.
  ([UpdateManager::wait_exit_then_apply_updates](https://docs.rs/velopack/1.2.161/velopack/struct.UpdateManager.html#method.wait_exit_then_apply_updates),
  [Integrating overview](https://docs.velopack.io/integrating/overview))
- `apply_updates_and_exit` and `apply_updates_and_restart` exit the program at once and apply
  the update; anything unsaved must be saved first.
  ([UpdateManager](https://docs.rs/velopack/1.2.161/velopack/struct.UpdateManager.html))

### 5. Building and packing on a GitHub Actions Windows runner

- Install the tool with `dotnet tool install -g vpk` (a .NET tool; the runner has .NET).
  ([GitHub Actions](https://docs.velopack.io/distributing/github-actions))
- `vpk download github --repoUrl <repo>` first puts the latest published release in the output
  folder, so `vpk pack` can make a delta package from it.
  ([GitHub Actions](https://docs.velopack.io/distributing/github-actions),
  [vpk options](https://docs.velopack.io/reference/cli/content/vpk-windows))
- `vpk pack --packId <id> --packVersion <version> --packDir <folder> --mainExe <exe>` packs the
  built program. The ID names the app for every later update, so it must stay the same. The
  version is semver 2 (`1.0.0`); four-part versions aren't supported. Other options used here:
  `--packTitle`, `--packAuthors`, `--icon`, `--releaseNotes`, `--outputDir`.
  ([Packaging overview](https://docs.velopack.io/packaging/overview),
  [vpk options](https://docs.velopack.io/reference/cli/content/vpk-windows))
- `vpk upload github --repoUrl <repo> --token <token> --publish --releaseName <name> --tag <tag>`
  creates the GitHub release and uploads the files; the workflow needs
  `permissions: contents: write`. The release's text is the release notes `vpk pack` put in the
  package (`--releaseNotes`), and it refuses a tag that already has a release.
  ([GitHub Actions](https://docs.velopack.io/distributing/github-actions),
  [vpk options](https://docs.velopack.io/reference/cli/content/vpk-windows),
  [vpk source, _GitRelease.cs](https://github.com/velopack/velopack/blob/1.2.161/src/vpk/Velopack.Deployment/_GitRelease.cs))
- `vpk pack` refuses an output folder that already holds a package of the same or a newer
  version than the one it packs, so only an older release may be downloaded into it first.
  ([vpk source, PackageBuilder.cs](https://github.com/velopack/velopack/blob/1.2.161/src/vpk/Velopack.Packaging/PackageBuilder.cs))
- Signing isn't required. An unsigned `Setup.exe` gets Windows' warning about an unknown
  publisher.
  ([Packaging overview](https://docs.velopack.io/packaging/overview),
  [Code signing](https://docs.velopack.io/packaging/signing))

## How this repo makes a release

Releases are made only when the user asks; no agent pushes a release tag.

1. Bump `version` in `app/Cargo.toml` (and `app/Cargo.lock` with it, e.g. `cargo check`) and
   merge that to main.
2. `tools/release-check.sh v0.2.0` must print the version (rule UP4: the tag is `v` plus the
   package version, `MAJOR.MINOR.PATCH`).
3. Tag that commit on main `v0.2.0` and push the tag. `.github/workflows/release.yml` checks the
   tag again, builds on Windows, packs `Setup.exe` with the release notes, and publishes it.

The release build links the C runtime into the program (`-C target-feature=+crt-static`), so it
starts on a PC without the Visual C++ runtime; the workflow fails if `nestpro.exe` still imports
`VCRUNTIME*.dll`.

**Run workflow** on the Release workflow (or `gh workflow run release.yml --ref <branch>`) is a
dry run: it builds the same installer as a workflow download and publishes nothing.

`tools/release-notes.sh` writes the notes (rule UP1): the changes since the previous tag, and
first **Changes machine programs**, the changes that touch the code that writes machine
programs. That code is the "Risky paths" list in `AGENTS.md`, which the script reads each time.
`tools/release-test.sh` tests both scripts.

## Updates on a PC of its own

A PC whose Library isn't a shop's checks GitHub Releases when the program opens and once a
day while it stays open (rules UP11–UP14 in `app/MECHANICS.md`). The check runs off the
window's thread and gives up after 30 seconds, finding out whether the PC is in a shop
included; with no connection the window shows no banner and no dialog, only "Couldn't check
for updates" on Settings → Shop → Updates. A PC in a shop never checks GitHub: it updates from
the shop folder (ADR 0006).

The program reads the release list itself (`updates.rs`, GitHub's releases API and each
release's `releases.win.json`), each request limited to what is left of the 30 seconds. It
doesn't use the crate's `GithubSource` for this: that one skips a release whose index can't be
downloaded or read, so the newest release could go missing and an older one be offered.

- **What is offered** is decided by `engine::updates::offer`: the newest plain
  `MAJOR.MINOR.PATCH` release newer than this PC's version, never a pre-release. Its notes'
  "Changes machine programs" entries are shown first.
- **Update when I close** downloads the full package in the background
  (`download_updates`, through `GithubSource`). When the window closes, the program waits up
  to 90 seconds for a download still running, looks again (within 30 seconds) that the PC isn't
  in a shop, and
  hands the downloaded package (`get_update_pending_restart`) to
  `wait_exit_then_apply_updates`, which installs once the program has ended. The program
  isn't restarted. A download not done in time installs nothing; the next start offers it
  again. Pointing the Library elsewhere, or it becoming a shop's, withdraws the choice.
- **Agents** see the same with `nestpro shop --mode=updates`; installing stays a person's
  choice, in the window.
- **Trying it without GitHub:** `--fake-releases=<file>` (or `NESTPRO_FAKE_RELEASES=<file>`)
  reads a made-up list instead, on any system: a JSON list of `{"version": "0.2.0",
  "notes": "..."}`. A file that isn't there plays no connection. `--update-chosen` and
  `--whats-new` show the banner after Update when I close, and What's new, for screenshots.
  Tests never read GitHub.

## Updates in a shop

A PC in a shop never checks GitHub by itself. The admin PC decides which release the shop runs
(ADR 0006, rules UP15–UP26 in `app/MECHANICS.md`):

- **Check GitHub** on the admin PC's Settings → Shop → Updates (a person's click) shows the
  newest published release, with What's new.
- **Try on this PC** needs a release to go back to: a shop with no shop version yet first rolls
  out the release this PC runs. It then fetches the tried release's index (`releases.win.json`) and full package into
  a release folder beside this PC's settings (`Releases\<version>`), takes a fresh backup of the
  shop and unpacks it into `Documents\Sheet Nesting\Trials\<shop> trying <version>`, which
  becomes this PC's Library. The release installs when the program closes, from that folder
  (`FileSource`, the exact version). Nothing the trial saves reaches the shop folder. When the
  tried release first opens the copy, its one-time changes to the shop's files run there.
  **Stop trying** points the Library back at the shop folder; the shop version installs when
  the program closes, and the shop is read-only on this PC until then.
- **Roll out to the shop** rolls out the release this PC runs, after a warning that names the
  PCs behind and a change of file layout. The package is checked against the release's index
  (`Size`, and `SHA256`, or `SHA1` when that's all it gives) and copied into a staging folder in
  `Updates` before the Library's write lock is taken. Under the lock, an index listing only that
  release is written, the folder is renamed to `Updates\<version>`, the release's one-time changes
  run on the shop, and it becomes the shop version, the one before kept beside it for roll back.
  Rolling the shop version out again copies only its package, when the one in `Updates` is
  missing or damaged. Agents are refused, and so is a roll out while a job from the trial copy is
  open.
- **Roll back to 0.4.0** makes the release kept beside the shop version the shop version again,
  after a warning that what only the newer release knew about a job may be lost when it is saved
  again. It is refused, changing nothing, when the shop file records a file layout the release
  kept can't read ("Jobs were saved in a layout 0.4.0 can't read. Restore a backup instead"),
  or when that release's index or package isn't whole in `Updates\<version>`. The shop file then
  lists the release left among those rolled back from, so the files it wrote save on the shop
  version again. Both releases stay in `Updates`. Every PC on the release left finds the shop
  read-only ("Restart to go back to 0.4.0") and goes back when its program closes: a downgrade,
  installed as below. Agents are refused.
- **Every other PC** reads the shop version when the program opens and at the Library's quick
  check. When its own version differs, it fetches that version from `Updates\<version>`
  (`FileSource`, with `AllowVersionDowngrade` when it goes back to an older shop version) and
  shows "Update ready, restart when you're done". It installs with `wait_exit_then_apply_updates`
  once the window has closed, and only while that release is still the shop version. The
  installer finds a downloaded release by itself only when it is newer
  (`get_update_pending_restart`), so the program keeps the release it fetched from the folder
  and hands that over.
- **Trying it without GitHub:** with `--fake-releases=<file>` a release is made up when it is
  fetched (an index and a package that are no program), so the whole flow can be shown on any
  system. `--shop-demo --rollout-demo=warning|ready|trial|rollback|rollback-refused` shows the
  warning before Roll out, "Update ready" on another PC, the trial banner, and the warning
  before Roll back or its refusal, on a made-up shop. Tests use made-up
  release folders.
