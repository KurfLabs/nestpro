//! A local-only comparison against the machine maker's own CAM (#128): each
//! nest of a folder of its `.bvc` files is opened as a job, written again on
//! a ByStar Fiber laser with a shop's switch values, and the two cut
//! programs are compared command by command.
//!
//! It runs only when both environment variables are set, and never names a
//! job, part or file in what it prints:
//!
//! - `NESTPRO_BYSOFT_NESTS`: a folder of `.bvc` files written by the CAM;
//! - `NESTPRO_BYSOFT_SWITCHES`: a JSON file of the machine's switch values as
//!   the `machine-switches` operation takes them (`set`, `mode`, `pierce`,
//!   and `cf1`, a post library file named relative to the JSON file).
//!
//! The inputs stay outside the repository; CI sets neither variable.

use super::super::*;
use std::collections::BTreeMap;

pub const NESTS: &str = "NESTPRO_BYSOFT_NESTS";
pub const SWITCHES: &str = "NESTPRO_BYSOFT_SWITCHES";

/// One command as the machine reads it: its name, its words (`Value`,
/// `Contour`) and its numbers in sheet mm (X, Y) and degrees (sweep).
#[derive(Clone, Debug)]
pub struct Cmd {
    pub name: String,
    pub word: String,
    pub xy: Option<P>,
    pub sweep: Option<f64>,
}

impl Cmd {
    fn same(&self, o: &Cmd) -> bool {
        let near = |a: Option<f64>, b: Option<f64>, t: f64| match (a, b) {
            (Some(a), Some(b)) => (a - b).abs() <= t,
            (None, None) => true,
            _ => false,
        };
        self.name == o.name
            && self.word == o.word
            && near(self.xy.map(|p| p[0]), o.xy.map(|p| p[0]), 0.01)
            && near(self.xy.map(|p| p[1]), o.xy.map(|p| p[1]), 0.01)
            && near(self.sweep, o.sweep, 0.01)
    }
}

/// One subprogram call as it runs on the sheet: whether it cuts a part or
/// cut-off lines, and its commands placed on the sheet.
#[derive(Clone, Debug)]
pub struct Run {
    pub cutoff: bool,
    pub cmds: Vec<Cmd>,
    /// The middle of its cut-outs' extent on the sheet (mm): where the part is.
    pub at: P,
}

/// The points a move to `to` passes (along its arc when it has a sweep).
/// Rotation keeps a sweep's sign, so the arc is the same on the sheet.
fn along(from: Option<P>, to: P, sweep: Option<f64>) -> Vec<P> {
    match (from, sweep) {
        (Some(f), Some(s)) => crate::bvc::arc_points(f, to, s),
        _ => vec![to],
    }
}

fn attrs(tag: &str) -> Vec<(String, String)> {
    let mut out = vec![];
    let mut r = tag;
    while let Some((k, v)) = r.split_once("=\"") {
        let Some((val, after)) = v.split_once('"') else { break };
        out.push((k.split_whitespace().last().unwrap_or("").to_string(), val.to_string()));
        r = after;
    }
    out
}

fn get<'a>(a: &'a [(String, String)], k: &str) -> Option<&'a str> {
    a.iter().find(|x| x.0 == k).map(|x| x.1.as_str())
}

/// A cut program as it runs: every call's commands on the sheet, in mm.
pub fn runs(text: &str) -> Vec<Run> {
    let head = text.find("<CuttingControlProgram").map(|a| &text[a..a + text[a..].find('>').unwrap_or(0)]).unwrap_or("");
    let ms = get(&attrs(head), "MeasurementSystem").unwrap_or("Metric").to_string();
    let k = if ms == "Metric" { 1.0 } else { 25.4 };
    // Subprograms by name (ignoring case): (has a part number, commands).
    let mut subs: BTreeMap<String, (bool, Vec<(String, Vec<(String, String)>)>)> = BTreeMap::new();
    let mut rest = text;
    while let Some(a) = rest.find("<SubProgram ") {
        let tag_end = a + rest[a..].find('>').unwrap();
        let sa = attrs(&rest[a..tag_end]);
        let end = tag_end + rest[tag_end..].find("</SubProgram>").unwrap_or(rest.len() - tag_end);
        let body = &rest[tag_end..end];
        let cmds = body.lines().filter_map(|l| {
            let l = l.trim();
            let b = l.strip_prefix('<')?.strip_suffix("/>")?.trim();
            let name = b.split_whitespace().next()?.to_string();
            Some((name, attrs(b)))
        });
        let name = get(&sa, "Name").unwrap_or("").to_lowercase();
        subs.insert(name, (get(&sa, "PartNumber").is_some(), cmds.collect()));
        rest = &rest[end..];
    }
    let main = text.rfind("</SubPrograms>").map_or("", |a| &text[a..]);
    let mut out = vec![];
    for l in main.lines() {
        let l = l.trim();
        let Some(b) = l.strip_prefix("<CallSubProgram") else { continue };
        let a = attrs(b);
        let num = |k: &str| get(&a, k).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
        let (x, y, r) = (num("X"), num("Y"), num("Rotation"));
        let Some((part, cmds)) = subs.get(&get(&a, "SubProgramName").unwrap_or("").to_lowercase()) else { continue };
        let (s, c) = r.to_radians().sin_cos();
        let place = |p: P| [(x + p[0] * c - p[1] * s) * k, (y + p[0] * s + p[1] * c) * k];
        let mut lo = [f64::MAX; 2];
        let mut hi = [f64::MIN; 2];
        let mut inside = false;
        let mut here: Option<P> = None;
        let mut v = vec![];
        for (name, at) in cmds {
            let xy = match (get(at, "X").and_then(|v| v.parse::<f64>().ok()), get(at, "Y").and_then(|v| v.parse::<f64>().ok())) {
                (Some(px), Some(py)) => Some(place([px, py])),
                _ => None,
            };
            match name.as_str() {
                // A cut-out starts where the head is.
                "BeginCutOut" => {
                    inside = true;
                    if let Some(p) = here {
                        lo = [lo[0].min(p[0]), lo[1].min(p[1])];
                        hi = [hi[0].max(p[0]), hi[1].max(p[1])];
                    }
                }
                "EndCutOut" => inside = false,
                _ => {}
            }
            if let (true, Some(p)) = (inside, xy) {
                let sweep = get(at, "SweepAngle").and_then(|v| v.parse::<f64>().ok());
                for q in along(here, p, sweep) {
                    lo = [lo[0].min(q[0]), lo[1].min(q[1])];
                    hi = [hi[0].max(q[0]), hi[1].max(q[1])];
                }
            }
            here = xy.or(here);
            let word = get(at, "Value").or(get(at, "Contour")).unwrap_or("").to_string();
            v.push(Cmd { name: name.clone(), word, xy, sweep: get(at, "SweepAngle").and_then(|v| v.parse().ok()) });
        }
        let at = if lo[0] <= hi[0] { [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0] } else { [0.0, 0.0] };
        out.push(Run { cutoff: !*part, cmds: v, at });
    }
    out
}

/// The commands of `a` and `b` that the other doesn't have, by an in-order
/// match (longest common run of equal commands).
pub fn unmatched(a: &[Cmd], b: &[Cmd]) -> (Vec<String>, Vec<String>) {
    let (n, m) = (a.len(), b.len());
    // Too large to match in full: compare position by position.
    if n.saturating_mul(m) > 40_000_000 {
        let mut x = vec![];
        let mut y = vec![];
        for i in 0..n.max(m) {
            match (a.get(i), b.get(i)) {
                (Some(p), Some(q)) if p.same(q) => {}
                (p, q) => {
                    x.extend(p.map(|c| c.name.clone()));
                    y.extend(q.map(|c| c.name.clone()));
                }
            }
        }
        return (x, y);
    }
    let mut t = vec![0u32; (n + 1) * (m + 1)];
    let at = |i: usize, j: usize| i * (m + 1) + j;
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            t[at(i, j)] = if a[i].same(&b[j]) { t[at(i + 1, j + 1)] + 1 } else { t[at(i + 1, j)].max(t[at(i, j + 1)]) };
        }
    }
    let (mut i, mut j) = (0, 0);
    let (mut x, mut y) = (vec![], vec![]);
    while i < n && j < m {
        if a[i].same(&b[j]) {
            i += 1;
            j += 1;
        } else if t[at(i + 1, j)] >= t[at(i, j + 1)] {
            x.push(a[i].name.clone());
            i += 1;
        } else {
            y.push(b[j].name.clone());
            j += 1;
        }
    }
    x.extend(a[i..].iter().map(|c| c.name.clone()));
    y.extend(b[j..].iter().map(|c| c.name.clone()));
    (x, y)
}

/// What one nest's comparison found: commands only the CAM's program has,
/// only ours has (by command name), part calls and contours cut in another
/// order.
#[derive(Default, Debug)]
pub struct Diff {
    pub theirs: BTreeMap<String, usize>,
    pub ours: BTreeMap<String, usize>,
    pub order: usize,
    pub contour_order: usize,
}

impl Diff {
    pub fn is_empty(&self) -> bool {
        self.theirs.is_empty() && self.ours.is_empty() && self.order == 0 && self.contour_order == 0
    }

    fn add(&mut self, x: Vec<String>, y: Vec<String>) {
        for c in x {
            *self.theirs.entry(c).or_default() += 1;
        }
        for c in y {
            *self.ours.entry(c).or_default() += 1;
        }
    }
}

/// A run's commands cut into contours: each from the commands that set it up
/// (mode, compensation, sensor) and its rapid to the next contour's, with
/// where it is (the middle of its cut-out's extent, else its rapid).
fn contours(cmds: &[Cmd]) -> Vec<(P, Vec<Cmd>)> {
    let mut starts: Vec<usize> = vec![];
    for (i, c) in cmds.iter().enumerate().filter(|c| c.1.name == "MoveLineTo") {
        let mut s = i;
        while s > 0 && matches!(cmds[s - 1].name.as_str(), "SetOperationMode" | "SetToolCorrection" | "SetSensingSensor") && !(cmds[s - 1].name == "SetToolCorrection" && cmds[s - 1].word == "None" && s - 1 > 0 && matches!(cmds[s - 2].name.as_str(), "EndCutOut" | "EndLeadOut" | "ExecHighPositioning")) {
            s -= 1;
        }
        let _ = c;
        starts.push(s);
    }
    if starts.first() != Some(&0) {
        starts.insert(0, 0);
    }
    starts.dedup();
    let mut out = vec![];
    for (k, &s) in starts.iter().enumerate() {
        let e = starts.get(k + 1).copied().unwrap_or(cmds.len());
        let block = cmds[s..e].to_vec();
        let (mut lo, mut hi, mut inside) = ([f64::MAX; 2], [f64::MIN; 2], false);
        let mut rapid = None;
        let mut here: Option<P> = None;
        for c in &block {
            match c.name.as_str() {
                // A cut-out starts where the head is.
                "BeginCutOut" => {
                    inside = true;
                    if let Some(p) = here {
                        lo = [lo[0].min(p[0]), lo[1].min(p[1])];
                        hi = [hi[0].max(p[0]), hi[1].max(p[1])];
                    }
                }
                "EndCutOut" => inside = false,
                "MoveLineTo" => rapid = rapid.or(c.xy),
                _ => {}
            }
            if let (true, Some(p)) = (inside, c.xy) {
                for q in along(here, p, c.sweep) {
                    lo = [lo[0].min(q[0]), lo[1].min(q[1])];
                    hi = [hi[0].max(q[0]), hi[1].max(q[1])];
                }
            }
            here = c.xy.or(here);
        }
        let at = if lo[0] <= hi[0] { [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0] } else { rapid.unwrap_or([0.0, 0.0]) };
        if !block.is_empty() {
            out.push((at, block));
        }
    }
    out
}

/// How many of `pairs` (in `a`'s order, `b`'s index) are out of `b`'s order:
/// those outside the longest run in order.
fn out_of_order(pairs: &[usize]) -> usize {
    let mut best: Vec<usize> = vec![];
    for &p in pairs {
        match best.binary_search(&p) {
            Ok(_) => {}
            Err(k) if k == best.len() => best.push(p),
            Err(k) => best[k] = p,
        }
    }
    pairs.len() - best.len()
}

/// Match `a`'s items with `b`'s by place (nearest unused within `tol` mm):
/// the pairs in `a`'s order and `b`'s items left over.
fn by_place<T>(a: &[(P, T)], b: &[(P, T)], tol: f64) -> (Vec<(usize, Option<usize>)>, Vec<usize>) {
    let mut used = vec![false; b.len()];
    let mut pairs = vec![];
    for (i, x) in a.iter().enumerate() {
        let best = (0..b.len()).filter(|&j| !used[j]).min_by(|&p, &q| dist(x.0, b[p].0).total_cmp(&dist(x.0, b[q].0))).filter(|&j| dist(x.0, b[j].0) <= tol);
        if let Some(j) = best {
            used[j] = true;
        }
        pairs.push((i, best));
    }
    (pairs, (0..b.len()).filter(|&j| !used[j]).collect())
}

/// Compare two runs' commands contour by contour: each contour matched with
/// the other's on the same place, compared command by command in order.
fn compare_contours(d: &mut Diff, a: &[Cmd], b: &[Cmd]) {
    let (ca, cb) = (contours(a), contours(b));
    let (pairs, left) = by_place(&ca, &cb, 0.5);
    let mut order = vec![];
    for (i, j) in pairs {
        match j {
            Some(j) => {
                order.push(j);
                let (x, y) = unmatched(&ca[i].1, &cb[j].1);
                d.add(x, y);
            }
            None => d.add(ca[i].1.iter().map(|c| c.name.clone()).collect(), vec![]),
        }
    }
    for j in left {
        d.add(vec![], cb[j].1.iter().map(|c| c.name.clone()).collect());
    }
    d.contour_order += out_of_order(&order);
}

/// Compare the CAM's cut program `theirs` with ours, command by command:
/// each part call matched with the one on the same place of the sheet and
/// compared contour by contour, the cut-off calls together.
pub fn compare(theirs: &str, ours: &str) -> Diff {
    let (a, b) = (runs(theirs), runs(ours));
    let mut d = Diff::default();
    let flat = |r: &[Run], off: bool| -> Vec<Cmd> { r.iter().filter(|x| x.cutoff == off).flat_map(|x| x.cmds.clone()).collect() };
    compare_contours(&mut d, &flat(&a, true), &flat(&b, true));
    let pa: Vec<(P, &Run)> = a.iter().filter(|r| !r.cutoff).map(|r| (r.at, r)).collect();
    let pb: Vec<(P, &Run)> = b.iter().filter(|r| !r.cutoff).map(|r| (r.at, r)).collect();
    let (pairs, left) = by_place(&pa, &pb, 1.0);
    let mut order = vec![];
    for (i, j) in pairs {
        match j {
            Some(j) => {
                order.push(j);
                compare_contours(&mut d, &pa[i].1.cmds, &pb[j].1.cmds);
            }
            None => d.add(pa[i].1.cmds.iter().map(|c| c.name.clone()).collect(), vec![]),
        }
    }
    for j in left {
        d.add(vec![], pb[j].1.cmds.iter().map(|c| c.name.clone()).collect());
    }
    d.order = out_of_order(&order);
    d
}

fn dist(a: P, b: P) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// What a refusal was about, in a few fixed words (never a name from it).
fn why(e: &str) -> &'static str {
    for (k, w) in [
        ("own leads", "micro-joints with their own leads"),
        ("corner loops", "corner loops"),
        ("kerf compensation", "planned without the machine's kerf compensation"),
        ("stop short", "cuts that stop short of their start"),
        ("wouldn't cut", "read-back mismatch"),
        ("point marks", "point marks"),
        ("switch", "a switch not followed yet"),
    ] {
        if e.contains(k) {
            return w;
        }
    }
    "other"
}

/// Run the comparison on `folder` with the switch values in `values`; the
/// summary as printed.
pub fn run(folder: &Path, values: &Path) -> String {
    // A folder of its own per run: two comparisons at once never share one.
    static RUNS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let d = dir(&format!("bysoft_compare_{}", RUNS.fetch_add(1, std::sync::atomic::Ordering::SeqCst)));
    let mut files: Vec<PathBuf> = std::fs::read_dir(folder).unwrap_or_else(|e| panic!("{NESTS}: {e}")).filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("bvc"))).collect();
    files.sort();
    let raw = std::fs::read_to_string(values).unwrap_or_else(|e| panic!("{SWITCHES}: {e}"));
    let mut args: Value = serde_json::from_str(&raw).unwrap_or_else(|e| panic!("{SWITCHES}: not JSON ({e})"));
    if let Some(c) = args.get("cf1").and_then(|c| c.as_str()).filter(|c| !c.is_empty()) {
        let p = values.parent().unwrap_or(Path::new(".")).join(c);
        args["cf1"] = json!(p.to_string_lossy());
    }
    args["machine"] = json!("Demo ByStar Fiber laser");
    let (mut nests, mut same, mut not_opened) = (0usize, 0usize, 0usize);
    let mut refused: BTreeMap<&str, usize> = BTreeMap::new();
    let mut total = Diff::default();
    let mut differing = 0usize;
    for (n, f) in files.iter().enumerate() {
        let mut s = Session::new(Ctx::builtin());
        s.ctx.cfg = bystar_settings(&d, true);
        call(&mut s, "machine-switches", None, args.clone());
        let bytes = std::fs::read(f).unwrap();
        let Ok(theirs) = crate::bvc::read(&bytes) else {
            not_opened += 1;
            continue;
        };
        let entries = crate::progimport::read_zip(&bytes).unwrap_or_default();
        let job = d.join(format!("NEST-{n}.snjob"));
        if s.call("import-program", Some(job.to_str().unwrap()), &json!({"path": f.to_str().unwrap()})).is_err() {
            not_opened += theirs.nests.len();
            continue;
        }
        let mut sheet = 1usize;
        for nest in &theirs.nests {
            nests += 1;
            let k = sheet;
            sheet += nest.plan.multiplicity.max(1) as usize;
            let want = format!("{}.ccp", nest.plan.name).to_lowercase();
            let Some(text) = entries.iter().find(|e| e.0.rsplit(['/', '\\']).next().unwrap_or("").to_lowercase() == want).map(|e| String::from_utf8_lossy(&e.1).to_string()) else { continue };
            match s.call("program", None, &json!({"sheet": k})) {
                Ok(r) => {
                    let ours = r.json["data"].as_str().unwrap_or("").to_string();
                    // Local debugging only: both programs into a folder outside the repo.
                    if let Some(dump) = std::env::var_os("NESTPRO_BYSOFT_DUMP") {
                        let dump = Path::new(&dump);
                        let _ = std::fs::create_dir_all(dump);
                        let _ = std::fs::write(dump.join(format!("{n}-{k}-cam.ccp")), &text);
                        let _ = std::fs::write(dump.join(format!("{n}-{k}-ours.ccp")), &ours);
                    }
                    let diff = compare(&text, &ours);
                    if diff.is_empty() {
                        same += 1;
                    } else {
                        differing += 1;
                    }
                    for (c, v) in diff.theirs {
                        *total.theirs.entry(c).or_default() += v;
                    }
                    for (c, v) in diff.ours {
                        *total.ours.entry(c).or_default() += v;
                    }
                    total.order += diff.order;
                    total.contour_order += diff.contour_order;
                }
                Err(e) => *refused.entry(why(&e)).or_default() += 1,
            }
        }
    }
    let _ = std::fs::remove_dir_all(&d);
    let mut out = String::new();
    let _ = writeln!(out, "Nests in the CAM's files: {}", nests + not_opened);
    let _ = writeln!(out, "Nests compared: {}", nests - refused.values().sum::<usize>());
    let _ = writeln!(out, "Identical, command for command: {same}");
    let _ = writeln!(out, "With differences: {differing}");
    let _ = writeln!(out, "Not opened: {not_opened}");
    for (w, c) in &refused {
        let _ = writeln!(out, "Not written ({w}): {c}");
    }
    let _ = writeln!(out, "Part calls cut in another order: {}", total.order);
    let _ = writeln!(out, "Contours cut in another order within a part: {}", total.contour_order);
    let mut names: Vec<&String> = total.theirs.keys().chain(total.ours.keys()).collect();
    names.sort();
    names.dedup();
    let _ = writeln!(out, "Differing commands by type (only in the CAM's program / only in ours):");
    for c in names {
        let _ = writeln!(out, "  {c}: {} / {}", total.theirs.get(c).unwrap_or(&0), total.ours.get(c).unwrap_or(&0));
    }
    out
}

use std::fmt::Write as _;

#[test]
fn the_cams_own_nests_written_again_with_a_shops_switch_values_compare_command_by_command() {
    let (Some(folder), Some(values)) = (std::env::var_os(NESTS), std::env::var_os(SWITCHES)) else {
        eprintln!("Skipped: the comparison with the machine maker's CAM runs only on a PC that has its nests. Set {NESTS} to a folder of its .bvc files and {SWITCHES} to a JSON file of the machine's switch values (as machine-switches takes them) to run it.");
        return;
    };
    let summary = run(Path::new(&folder), Path::new(&values));
    println!("{summary}");
}

#[test]
fn two_cut_programs_compare_command_by_command_whatever_their_names_and_call_order() {
    // Made-up: one part cut by subprogram "A" at 10,10 and the cut-off; the
    // other program names it "1", calls it second and has one move more.
    let prog = |sub: &str, extra: bool, cut_first: bool| {
        let part = format!(
            "<SubProgram Id=\"1\" Name=\"{sub}\" PartNumber=\"1\"><ControlCommands>\r\n<MoveLineTo Id=\"2\" X=\"0\" Y=\"0\" />\r\n<BeginCutOut Id=\"3\" Contour=\"Exterior\" />\r\n<LineTo Id=\"4\" X=\"10\" Y=\"0\" />\r\n{}<LineTo Id=\"5\" X=\"0\" Y=\"0\" />\r\n<EndCutOut Id=\"6\" />\r\n</ControlCommands></SubProgram>\r\n",
            if extra { "<LineTo Id=\"9\" X=\"5\" Y=\"0\" />\r\n" } else { "" }
        );
        let off = "<SubProgram Id=\"7\" Name=\"ResidualSheetCuts\"><ControlCommands>\r\n<MoveLineTo Id=\"8\" X=\"50\" Y=\"0\" />\r\n</ControlCommands></SubProgram>\r\n";
        let calls = [format!("<CallSubProgram Id=\"10\" X=\"10\" Y=\"10\" Rotation=\"0\" SubProgramName=\"{sub}\" />\r\n"), "<CallSubProgram Id=\"11\" X=\"0\" Y=\"0\" Rotation=\"0\" SubProgramName=\"ResidualSheetCuts\" />\r\n".to_string()];
        let main = if cut_first { calls.concat() } else { format!("{}{}", calls[1], calls[0]) };
        format!("<CuttingControlProgram MeasurementSystem=\"Metric\">\r\n<SubPrograms>\r\n{part}{off}</SubPrograms>\r\n<ControlCommands>\r\n{main}</ControlCommands>\r\n</CuttingControlProgram>\r\n")
    };
    let d = compare(&prog("A", false, true), &prog("1", false, false));
    assert!(d.is_empty(), "{d:?}");
    let d = compare(&prog("A", false, true), &prog("1", true, true));
    assert_eq!((d.theirs.len(), d.ours.get("LineTo")), (0, Some(&1)), "{d:?}");
    // An inch program is compared in mm.
    let inch = prog("A", false, true).replace("Metric", "English").replace("X=\"10\" Y=\"0\"", "X=\"0.393700787\" Y=\"0\"").replace("X=\"10\" Y=\"10\" Rotation", "X=\"0.393700787\" Y=\"0.393700787\" Rotation").replace("X=\"50\"", "X=\"1.968503937\"");
    let d = compare(&inch, &prog("1", false, true));
    assert!(d.is_empty(), "{d:?}");
}
