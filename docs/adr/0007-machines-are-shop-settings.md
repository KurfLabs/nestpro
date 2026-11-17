# Machines are shop settings, the same on every PC

A saved job exported on any PC in a shop must give the same machine program. Today each PC keeps its own
machines, post libraries and technology tables, pointing into its own copy of the original program,
so two PCs can quietly write different programs for the same job.

Settings come in three kinds:

- **Shop only**: the same on every PC and never changed on one PC alone. Everything that turns a
  saved job into a machine program is here: machines, post libraries, technology tables, gases,
  materials, and the ERP set-up.
- **Shop default**: the shop's value, which a PC may replace with its **own value**, such as units,
  part gap and edge margin for new jobs, standard drop sizes, or where programs are saved. These
  shape how a new job is nested; once saved, the job exports the same on every PC.
- **PC setting**: never shared, such as mouse and keys, the window, which shop folder to use, or a
  programmer's ERP token.

A PC that needs a different post or technology table gets another machine in the shop's list, not a
hidden own value.

## Considered options

- **Every setting may have an own value.** Rejected: an own value on a post or technology table is
  exactly the silent difference between PCs this is meant to end.
- **Every setting is the shop's.** Rejected: units, folders and mouse habits are personal and change
  no program.

## Consequences

- Starting a shop copies the admin PC's machine files into the shop folder; every PC then uses those
  copies.
- The shop chooses whether shop settings are changed from any PC or only on the admin PC. Each
  change records who made it and when.
