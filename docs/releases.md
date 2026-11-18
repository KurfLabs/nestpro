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
