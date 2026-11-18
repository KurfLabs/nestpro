# Setting up a shop

A **shop** is the programming PCs that share one Library: the jobs, job folders, library parts,
sheet stock and material list every PC works from. The Library is a folder on the shop's Windows
share (ADR 0003), and that folder is the **shop folder**. This guide sets one up for a made-up shop
called Acme, whose server is `srv` and whose share is `\\srv\Nesting`. Use your own names.

What the program does at each step is in `app/MECHANICS.md`, rules SH1–SH16.

## 1. Make the share

On the server (or the PC that is always on and holds the shop's files):

1. Make a folder for the shop, for example `D:\Nesting`.
2. Share it: right-click the folder → **Properties** → **Sharing** → **Advanced Sharing** →
   **Share this folder**, share name `Nesting`. The folder is now `\\srv\Nesting` on the network.
3. Under **Permissions** on the same window, give **Everyone** (or **Authenticated Users**)
   **Change** and **Read**. Who may change what is decided by the folder's own rights (step 2
   below), not the share's.

If the shop already shares a Library folder, that folder becomes the shop folder where it is.
Nothing moves, and every PC keeps its path.

## 2. Give a "Nesting" group Modify rights

Every programming PC's Windows user must be able to make, change and remove files in the shop
folder, not only write them: saving a job writes a new file and renames it over the old one, and
the Library's write lock is a file each PC makes and removes.

1. On the server, make a local or domain group called `Nesting`, and add each programmer's Windows
   user to it.
2. Right-click `D:\Nesting` → **Properties** → **Security** → **Edit** → **Add** → `Nesting` →
   tick **Modify** (Allow). Apply.

When a PC can't make or remove files there, joining says so and names the folder: "the shop's
group needs Modify rights".

## 3. Use the full `\\server\share` path

Point every PC at `\\srv\Nesting`, not at a drive letter such as `Z:\`. A drive letter is set up on
each PC by itself and can differ from PC to PC; the full path is the same everywhere, so every PC
names the same Library.

If you pick the folder through a mapped drive anyway, the program uses the `\\srv\...` path behind
the drive letter when Windows gives one, and says so: "Z: is \\srv\Nesting on this PC: using
\\srv\Nesting, so every PC names the same Library."

## 4. Install the program on each PC

Download `Setup.exe` from the release on GitHub and run it on each programming PC. It installs for
the Windows user who runs it and needs no administrator rights. Windows shows an "unknown
publisher" warning, because the installer isn't signed yet: choose **More info** → **Run anyway**.
`docs/releases.md` says more about releases.

## 5. Start the shop on the admin PC, then join it from the others

The first time the program opens on a PC with no settings, it asks where that PC keeps its work:

- **Start a shop** — on the PC that will be the **admin PC** (the one that rolls out updates and
  restores backups): pick `\\srv\Nesting` and type the shop's name, `Acme`. If the folder holds no
  Library yet, the program asks before it makes a new, empty one there.
- **Join a shop** — on every other PC: pick `\\srv\Nesting`. Before it says yes, the program checks
  that the folder can be reached, that it holds a Library and a shop, that this PC can take the
  Library's write lock there and make, change and remove a file there, and compares this PC's
  version with the shop's. A PC on an older version than the shop's may join and is told to update.
  Each check is shown; a refusal says what to fix, and nothing is made in the folder.
- **Just this PC** — a Library on this PC only, as before. It can join a shop later.

Until one of the three is done, nothing is saved: close the program first and the screen comes
back next time.

A PC that already has a Library set skips this screen: use **Settings → Shop** (Start a shop here,
or Join a shop…). Joining leaves the PC's own Library as it was on disk; its materials join the
shop's material list. A PC that is already in another shop is asked first, naming the shop it
leaves.

To set up several PCs from a script:

```
nestpro setup --join \\srv\Nesting --text
```

It prints each check, one per line, and exits with a non-zero code when it refuses. A PC already
in another shop is refused unless you add `--leave`. The admin PC can start the shop the same way:
`nestpro setup --start \\srv\Nesting --name Acme`.

`nestpro shop --text` (or Settings → Shop) lists the shop's PCs, who used each last, its version
and when it was last seen.

## Joining over the VPN

A programmer working from home joins the same way, through the VPN:

1. Connect the VPN first, and check that `\\srv\Nesting` opens in File Explorer.
2. If the name `srv` isn't found over the VPN, use the server's full name (`\\srv.acme.example\Nesting`)
   or ask whoever runs the VPN to make the short name work. Use the same name on that PC every time.
3. Join `\\srv\Nesting` as above. The checks tell you at once if the folder can't be reached or
   written, rather than at your first save.

When the VPN is down, the program says the Library on `\\srv\Nesting` can't be reached and names
it. It never makes a new, empty Library in its place: connect again and choose Try again.
