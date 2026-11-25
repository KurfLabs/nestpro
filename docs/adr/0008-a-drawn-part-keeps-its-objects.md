# A drawn part keeps its objects; its contours are built from them

Parts can now be drawn in the app, with an AutoCAD-style command line on the Part screen. Today every part
is kept as point lists: an importer turns each arc into short straight pieces at 0.1 mm, and the
nesting, kerf, leads, joints and posts all work on those points. A drawing tool needs the real
lines, arcs and circles. Snaps to a tangent or a centre, Offset, Trim and Fillet, and an exact DXF
all need them, and point lists can't give them back reliably.

We considered making every part exact: storing arcs on each contour and teaching nesting, kerf,
leads and the posts to use them. We chose to keep the point lists as the one geometry everything
downstream reads, and to add the exact **objects** (lines, arcs, circles and polylines with arcs)
only to a part drawn in the app. The objects are the source; the part's contours are **built from
them** by the same code that builds an imported drawing, and with the importer's arc tolerance. A
drawn part is therefore cut exactly like the same shape exported as a DXF and imported, and drawing
changes no machine program.

## Consequences

- A drawn part saves its objects beside its contours (`#[serde(default)]`). Jobs and Library
  products saved before this open unchanged. A part that was imported or saved before this has no
  objects, and its points are never fitted back into arcs.
- The contours are built only from objects that pass a strict check: one closed outline, holes
  wholly inside it that don't touch, and no crossings, branches or open ends among the cut lines. The
  importer's gap closing never hides a mistake in a drawn part. The check names what is wrong and
  where.
- A drawing that doesn't pass yet is a **draft**. A job keeps its drafts apart from its parts, so a
  draft is never nested, costed or cut.
- Anything that changes a drawn part's points, such as Turn left or Turn right, changes its objects
  the same way, so the two never drift apart.
- An older program never saves over a job written by a newer one (UP7), so it can't silently drop
  the objects.
- Importers keep turning arcs into points for now. Editing an imported drawing with the drawing
  commands, and keeping its true arcs, is later work.
- Exact arcs stop at the contour. Kerf, leads and the posts keep working on points and fitting arcs
  back. Cutting from exact arcs would be its own decision.
