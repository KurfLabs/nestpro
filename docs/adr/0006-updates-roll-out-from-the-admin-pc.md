# Updates roll out from the admin PC through the shop folder

Every PC in a shop reads and writes the same Library, so they must run versions that agree on its
files. If each PC updated itself from the internet whenever a release came out, a shop would run a
mix of versions picked by chance, a PC could update in the middle of a shift, and an older PC could
save over a job written by a newer one and lose what it doesn't know about.

So one PC, the **admin PC**, decides. It tries a release first on a copy of the shop, so nothing
the trial saves reaches the shop, then **rolls it out**:
the release is copied into the shop folder and named the **shop version**. Every other PC installs
it from there the next time the program closes. A PC not in a shop updates itself from the public
releases, and asks first.

## Considered options

- **Each PC updates itself from the internet.** Rejected: versions drift apart, and nobody chooses
  when a change that alters machine programs reaches the shop.
- **An update server in the shop.** Rejected for the same reason as a database server (ADR 0003):
  something to install and look after, for a handful of PCs that already share a folder.
- **A per-machine installer pushed out by IT.** Kept for later. The per-user installer needs no
  administrator rights, and the shop has no IT department to push one.

## Consequences

- PCs in a shop need no internet access; only the admin PC fetches releases.
- A program never saves over a Library file last written by a newer version. It opens the file
  read-only and says to update this PC.
- A one-time change a release makes to the shop's files (such as moving machines into the shop) is
  made on the trial copy during a trial, and on the shop itself only when the release is rolled out.
- Roll back only goes back to a release that can read the shop's files as they are now; otherwise a
  backup is restored instead. Every release the shop has rolled back from is recorded, and files
  written by any of them count as the shop version's, so PCs can save them again; what only the newer release knew about them may be
  lost, and Roll back says so before it goes ahead.
- Release notes say plainly when a release changes the programs a machine gets, so the shop
  test-cuts before rolling it out.
