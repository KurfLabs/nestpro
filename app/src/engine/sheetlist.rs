//! A job's own list of sheets to nest on (JS1–JS6): sizes, stock sheets,
//! remnants and drops the programmer adds to the job (Nesting Explorer →
//! right-click the job → Add sheet size…, agents' `sheet-list`). Nest all
//! fills them first, in the list's order, then goes on as the Do All options
//! say (the stock, then the job's sheet size). Shared by the window and the
//! agent operations; the list itself is `SheetSpec::sheet_list`.

use crate::nest::{FormatSlot, ListSheet, SheetFormat, SheetSpec};
use crate::stock::{self, Stock, StockSettings};
use std::collections::BTreeMap;

/// Most sheets one line may ask for.
pub const MAX_QTY: u32 = 999;

/// Add a line to the job's list (JS1). The same sheet again (the same stock
/// item, or the same size of its own) adds to that line's quantity instead
/// of making a second line; "as many as needed" on either keeps it so.
/// Refused when the sheet is too small for the edge margin. A stock item is
/// checked by the caller (`check_stock_item`). Returns what was done.
pub fn add(spec: &mut SheetSpec, line: ListSheet) -> Result<String, String> {
    if !(line.w > 0.0 && line.h > 0.0) {
        return Err("Give a length and a width above 0.".into());
    }
    if line.w <= 2.0 * spec.margin || line.h <= 2.0 * spec.margin {
        return Err(format!("A {} × {} mm sheet has no room inside the {} mm edge margin.", num(line.w), num(line.h), num(spec.margin)));
    }
    if line.qty == Some(0) {
        return Err("Give a quantity of 1 or more, or as many as needed.".into());
    }
    let n = spec.sheet_list.len();
    if let Some((i, l)) = spec.sheet_list.iter_mut().enumerate().find(|(_, l)| l.same(&line)) {
        l.qty = match (l.qty, line.qty) {
            (Some(a), Some(b)) => Some((a + b).min(MAX_QTY)),
            _ => None,
        };
        return Ok(format!("Line {} of the job's sheet list is now {}.", i + 1, describe(l)));
    }
    let what = describe(&line);
    spec.sheet_list.push(line);
    Ok(format!("Added to the job's sheet list as line {}: {what}.", n + 1))
}

/// Can this stock item go on the job's list (JS4)? Its material and thickness
/// must be the job's, and it mustn't be locked (locked sheets are never used
/// by automatic nesting, SS4).
pub fn check_stock_item(spec: &SheetSpec, it: &stock::Item) -> Result<(), String> {
    if it.material != spec.material || (it.thickness - spec.thickness).abs() > stock::TOL {
        return Err(format!(
            "'{}' is {} {} mm; the job is {} {} mm.",
            it.reference,
            super::material::name_or(it.material, "?"),
            num(it.thickness),
            super::material::name_or(spec.material, "?"),
            num(spec.thickness)
        ));
    }
    if it.locked {
        return Err(format!("'{}' is locked in the sheet stock, so nesting never uses it. Unlock it in Sheet stock first.", it.reference));
    }
    Ok(())
}

/// Take line `i` (from 0) off the list.
pub fn remove(spec: &mut SheetSpec, i: usize) -> Result<ListSheet, String> {
    line_at(spec, i)?;
    Ok(spec.sheet_list.remove(i))
}

/// Change how many sheets line `i` asks for (None = as many as needed).
pub fn set_qty(spec: &mut SheetSpec, i: usize, qty: Option<u32>) -> Result<(), String> {
    line_at(spec, i)?;
    if qty == Some(0) {
        return Err("Give a quantity of 1 or more, or as many as needed. To take the line off, remove it.".into());
    }
    spec.sheet_list[i].qty = qty.map(|q| q.min(MAX_QTY));
    Ok(())
}

/// Move line `i` to place `to` (both from 0): the list's order is the order
/// Nest all takes the sheets in (JS3).
pub fn move_to(spec: &mut SheetSpec, i: usize, to: usize) -> Result<(), String> {
    line_at(spec, i)?;
    let to = to.min(spec.sheet_list.len() - 1);
    let l = spec.sheet_list.remove(i);
    spec.sheet_list.insert(to, l);
    Ok(())
}

fn line_at(spec: &SheetSpec, i: usize) -> Result<&ListSheet, String> {
    spec.sheet_list.get(i).ok_or_else(|| match spec.sheet_list.len() {
        0 => "The job's sheet list is empty.".to_string(),
        n => format!("The job's sheet list has {n} line(s); give one from 1 to {n}."),
    })
}

/// "1524 × 1219 mm, as many as needed" / "DROP-12 (remnant, 1219 × 610 mm), 2 sheets".
pub fn describe(l: &ListSheet) -> String {
    let size = format!("{} × {} mm", num(l.w), num(l.h));
    let what = if l.stock_ref.is_empty() { size } else { format!("{} ({}, {size})", l.stock_ref, if l.remnant { "remnant" } else { "stock sheet" }) };
    format!("{what}, {}", qty_text(l.qty))
}

/// "as many as needed", "1 sheet", "3 sheets".
pub fn qty_text(q: Option<u32>) -> String {
    match q {
        None => "as many as needed".into(),
        Some(1) => "1 sheet".into(),
        Some(n) => format!("{n} sheets"),
    }
}

fn num(v: f64) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// The job's list as nesting slots, in its order (JS3, JS4), and a note for
/// each line left out. A size of its own is a plain sheet of that size. A
/// stock line is read from the stock now (its real outline, a remnant turned
/// for the machine as stock nesting turns it): at most what the stock has
/// plus what this job already holds of it (SS12), unless the options allow
/// sheets with too little stock; left out when it is gone, locked, of another
/// material or thickness, one of this job's own remnants (SA1), or when the
/// stock can't be read (`stock` None).
pub fn slots(spec: &SheetSpec, stock: Option<&Stock>, held: &BTreeMap<String, i32>, st: &StockSettings, exclude: &[String]) -> (Vec<FormatSlot>, Vec<String>) {
    let mut out = vec![];
    let mut notes = vec![];
    for (k, l) in spec.sheet_list.iter().enumerate() {
        let qty = l.qty.map(|q| q as usize);
        if l.stock_ref.is_empty() {
            out.push(FormatSlot { format: SheetFormat::rect(l.w, l.h), qty });
            continue;
        }
        let r = &l.stock_ref;
        let skip = |why: String| format!("Line {} ({r}) of the job's sheet list wasn't used: {why}", k + 1);
        let Some(stock) = stock else {
            notes.push(skip("the sheet stock can't be read.".into()));
            continue;
        };
        let Some(it) = stock.get(r) else {
            notes.push(skip("it's no longer in the sheet stock.".into()));
            continue;
        };
        if let Err(e) = check_stock_item(spec, it) {
            notes.push(skip(e));
            continue;
        }
        if exclude.contains(r) {
            notes.push(skip("it's a remnant this job cut itself.".into()));
            continue;
        }
        let free = (it.qty + held.get(r).copied().unwrap_or(0)).max(0) as usize;
        let qty = if st.allow_no_stock {
            qty
        } else {
            if free == 0 {
                notes.push(skip("none of it is left in stock.".into()));
                continue;
            }
            Some(qty.map_or(free, |q| q.min(free)))
        };
        let mut f = SheetFormat::from_stock(it);
        // A remnant is turned for the machine as it's loaded (RE26).
        if let Some(o) = f.outline.as_ref().filter(|_| f.remnant) {
            let o = stock::orient_for_machine(o, st.remnant_rotation, st.turn_over);
            (f.w, f.h) = o.size();
            f.outline = Some(o);
        }
        out.push(FormatSlot { format: f, qty });
    }
    (out, notes)
}

/// The stock items the list takes, so the stock's own order (after the
/// list) offers only what is left of them (JS3): each slot's quantity is
/// taken off its item's; a line with no limit leaves none.
pub fn take_from(plan: &mut [FormatSlot], list: &[FormatSlot]) {
    for l in list.iter().filter(|l| !l.format.stock_ref.is_empty()) {
        for s in plan.iter_mut().filter(|s| s.format.stock_ref == l.format.stock_ref) {
            s.qty = match (s.qty, l.qty) {
                (_, None) => Some(0),
                (None, Some(_)) => None,
                (Some(a), Some(b)) => Some(a.saturating_sub(b)),
            };
        }
    }
}

/// The list for agents (`info`, `sheet-list`).
pub fn to_json(spec: &SheetSpec) -> serde_json::Value {
    serde_json::Value::Array(
        spec.sheet_list
            .iter()
            .enumerate()
            .map(|(k, l)| {
                serde_json::json!({
                    "line": k + 1,
                    "w": l.w,
                    "h": l.h,
                    "ref": l.stock_ref,
                    "remnant": l.remnant,
                    "qty": l.qty,
                    "text": describe(l),
                })
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> SheetSpec {
        SheetSpec { w: 3000.0, h: 1500.0, margin: 10.0, gap: 5.0, material: 0, thickness: 3.0, level: 3, machine: Default::default(), kerf: Default::default(), gas: None, pick: Default::default(), sheet_list: vec![] }
    }

    #[test]
    fn the_same_sheet_again_adds_to_its_line() {
        // JS1: same size of its own → one line; as many as needed stays so.
        let mut s = spec();
        add(&mut s, ListSheet::size(1200.0, 800.0, Some(1))).unwrap();
        add(&mut s, ListSheet::size(1200.02, 800.0, Some(2))).unwrap();
        assert_eq!((s.sheet_list.len(), s.sheet_list[0].qty), (1, Some(3)));
        add(&mut s, ListSheet::size(1200.0, 800.0, None)).unwrap();
        assert_eq!(s.sheet_list[0].qty, None);
        add(&mut s, ListSheet::size(800.0, 1200.0, Some(1))).unwrap();
        assert_eq!(s.sheet_list.len(), 2, "turned is another size");
        move_to(&mut s, 1, 0).unwrap();
        assert_eq!(s.sheet_list[0].w, 800.0);
        assert!(set_qty(&mut s, 0, Some(0)).is_err());
        assert!(remove(&mut s, 5).is_err());
    }

    #[test]
    fn a_new_job_starts_without_the_last_jobs_list() {
        // JS1: a new job is made from the open job's sheet, but not its list.
        let mut s = spec();
        add(&mut s, ListSheet::size(1200.0, 800.0, Some(1))).unwrap();
        assert!(super::super::with_prefs(&s, &crate::settings::AppSettings::default()).sheet_list.is_empty());
    }

    #[test]
    fn the_stock_offers_only_what_the_list_leaves() {
        // JS3: a listed stock item's sheets are taken off its stock slot.
        let f = |r: &str| SheetFormat { stock_ref: r.into(), ..SheetFormat::rect(1000.0, 500.0) };
        let mut plan = vec![FormatSlot { format: f("D1"), qty: Some(3) }, FormatSlot { format: f("D2"), qty: Some(2) }, FormatSlot { format: f(""), qty: None }];
        take_from(&mut plan, &[FormatSlot { format: f("D1"), qty: Some(2) }, FormatSlot { format: f("D2"), qty: None }]);
        assert_eq!(plan.iter().map(|s| s.qty).collect::<Vec<_>>(), vec![Some(1), Some(0), None]);
    }
}
