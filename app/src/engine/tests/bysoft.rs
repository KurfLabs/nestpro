//! ByStar Fiber laser: this app's own switches for the machine maker's CAM
//! form (BV67–BV72, ticket #128), tested as an agent drives them.
//!
//! The golden files (`testdata/bystar-off-*`) are what spec #122 wrote
//! before these switches existed (#127): with every one of them off, the
//! files written must stay byte for byte the same.

use super::*;

/// The made-up machine's switches set to `set` ("X002=1, B001=1").
fn set(s: &mut Session, set: &str) {
    switches(s, set);
}

/// The made-up job DEMO-FORM on the ByStar Fiber laser: three 200 × 100
/// plates placed on half-millimetre positions (x.5 rounds toward zero),
/// one turned 90° and one 180°, an engraved label next to the first plate's
/// holes, no lead-ins on the second plate, micro-joints on the third; sheet
/// 2 the same as sheet 1 (one nest run twice), sheet 3 one plate with a
/// remnant cut-off.
fn form_job(d: &Path) -> Session {
    std::fs::create_dir_all(d).unwrap();
    let mut s = Session::new(Ctx::builtin());
    s.ctx.cfg = bystar_settings(d, true);
    let job = d.join("DEMO-FORM.snjob");
    call(&mut s, "new", Some(job.to_str().unwrap()), json!({"w": 1000, "h": 500, "material": "mild steel", "thickness": 3}));
    for n in ["DEMO-001", "DEMO-002", "DEMO-003"] {
        call(&mut s, "import-drawing", None, json!({"path": plate_with_holes_and_slot(d, false, n).to_str().unwrap(), "qty": 1}));
    }
    call(&mut s, "text", None, json!({"mode": "add", "parts": 1, "text": "DEMO", "height": 12, "kind": "mark"}));
    call(&mut s, "leads", None, json!({"mode": "auto"}));
    call(&mut s, "leads", None, json!({"mode": "delete", "parts": 2}));
    call(&mut s, "joints", None, json!({"mode": "auto", "every": 120, "parts": 3}));
    for k in ["1", "2"] {
        let first = if k == "1" { json!({"part": 1, "x": 50.5, "y": 20.5}) } else { json!({"part": 1, "sheet": "new", "x": 50.5, "y": 20.5}) };
        call(&mut s, "place", None, first);
        call(&mut s, "place", None, json!({"part": 2, "sheet": k, "x": 400.5, "y": 50, "angle": 90}));
        call(&mut s, "place", None, json!({"part": 3, "sheet": k, "x": 600, "y": 300.5, "angle": 180}));
    }
    call(&mut s, "place", None, json!({"part": 1, "sheet": "new", "x": 12.5, "y": 12.5}));
    call(&mut s, "cut-line", None, json!({"sheet": 3, "points": "600,0;600,500"}));
    s
}

/// The proof job of #129 (testdata), opened in a session on the made-up machines.
fn proof_job(d: &Path) -> Session {
    let stored = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/engine/testdata/bystar-proof.snjob");
    std::fs::create_dir_all(d).unwrap();
    let mut s = Session::new(Ctx::builtin());
    s.ctx.cfg = bystar_settings(d, true);
    let job = d.join("DEMO-PROOF.snjob");
    std::fs::copy(&stored, &job).unwrap();
    call(&mut s, "info", Some(job.to_str().unwrap()), json!({}));
    s
}

/// An inch post library (CF1 A006 = 25.4) for the made-up machine, with
/// B001 (the inch text) and whole-inch cut-off distances.
fn inch(s: &mut Session, d: &Path) {
    let cf1 = d.join("DEMO-INCH.CF1");
    std::fs::write(&cf1, "A005 0.005 ; tolerance\nA006 25.4 ; inch\n").unwrap();
    call(s, "machine-switches", None, json!({"machine": "Demo ByStar Fiber laser", "cf1": cf1.to_str().unwrap()}));
    set(s, "B001=1, B003=1, B004=1, B005=1");
}

/// Every file a job's export writes to its ByStar Fiber folder, the `.bvc`
/// opened into its entries; the job file's new Guid made fixed.
fn bystar_files_written(s: &mut Session, d: &Path, job: &str) -> Vec<(String, Vec<u8>)> {
    let folder = bystar_dir(d, job);
    let _ = std::fs::remove_dir_all(&folder);
    call(s, "export", None, json!({}));
    let mut v = vec![];
    let mut names: Vec<PathBuf> = std::fs::read_dir(&folder).unwrap().map(|e| e.unwrap().path()).collect();
    names.sort();
    for p in names {
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        let bytes = std::fs::read(&p).unwrap();
        if name.ends_with(".bvc") {
            for (inner, b) in crate::progimport::read_zip(&bytes).unwrap() {
                v.push((format!("{name}!{inner}"), fixed_guid(b)));
            }
        } else {
            v.push((name, fixed_guid(bytes)));
        }
    }
    v
}

/// A job file's Guid (new for every write, BV43) made `0…0`.
fn fixed_guid(b: Vec<u8>) -> Vec<u8> {
    let Ok(t) = String::from_utf8(b.clone()) else { return b };
    match t.find("Guid=\"") {
        Some(a) => {
            let a = a + 6;
            let e = a + t[a..].find('"').unwrap();
            format!("{}00000000-0000-0000-0000-000000000000{}", &t[..a], &t[e..]).into_bytes()
        }
        None => b,
    }
}

/// The made-up jobs the golden files hold, each with the switches it is
/// written with (the post's own, none of this app's).
fn golden_cases(d: &Path) -> Vec<(&'static str, Session, &'static str)> {
    let mut out = vec![];
    let s = proof_job(&d.join("proof"));
    out.push(("proof", s, "DEMO-PROOF"));
    let mut s = proof_job(&d.join("proof-inch"));
    inch(&mut s, &d.join("proof-inch"));
    out.push(("proof-inch", s, "DEMO-PROOF"));
    let s = form_job(&d.join("form"));
    out.push(("form", s, "DEMO-FORM"));
    let mut s = form_job(&d.join("form-inch"));
    inch(&mut s, &d.join("form-inch"));
    set(&mut s, "B021=1");
    out.push(("form-inch", s, "DEMO-FORM"));
    out
}

#[test]
fn with_this_apps_own_switches_off_the_files_are_byte_for_byte_what_127_wrote() {
    let d = dir("bysoft_golden");
    let cases = golden_cases(&d);
    for (name, mut s, job) in cases {
        let sub = d.join(name);
        let files = bystar_files_written(&mut s, &sub, job);
        assert!(files.iter().any(|f| f.0.ends_with(".ccp")), "{name}: {:?}", files.iter().map(|f| &f.0).collect::<Vec<_>>());
        let list: Vec<String> = files.iter().map(|f| f.0.clone()).collect();
        same_as_recorded(&format!("bystar-off-{name}-files.txt"), &(list.join("\n") + "\n"));
        for (f, bytes) in &files {
            let safe = f.replace('!', "--");
            same_as_recorded(&format!("bystar-off-{name}-{safe}"), &String::from_utf8(bytes.clone()).unwrap());
        }
    }
    let _ = std::fs::remove_dir_all(&d);
}

mod compare;

/// Each contour of a cut program: its commands from its rapid (with the
/// mode and compensation right before it) to the `SetToolCorrection None`
/// after it, as names.
fn blocks(ccp: &str) -> Vec<Vec<String>> {
    let cmds = ccp_commands(ccp);
    let n = cmds.iter().filter(|c| c.0 == "BeginCutOut").count();
    (0..n).map(|k| contour_block(&cmds, k)).collect()
}

#[test]
fn x002_writes_the_lead_out_after_the_cut_out_ends() {
    let d = dir("bysoft_x002");
    let mut s = bystar_job(&d, false);
    set(&mut s, "X002=1");
    let ccp = bystar_program_text(&mut s, 1);
    let with_out: Vec<Vec<String>> = blocks(&ccp).into_iter().filter(|b| b.iter().any(|c| c == "BeginLeadOut")).collect();
    assert!(!with_out.is_empty(), "{ccp}");
    for b in &with_out {
        let at = |n: &str| b.iter().position(|c| c == n).unwrap();
        // BV67: EndCutOut, then the lead-out in its bracket, then compensation off.
        assert!(at("EndCutOut") < at("BeginLeadOut") && at("BeginLeadOut") < at("EndLeadOut"), "{b:?}");
        assert_eq!(&b[b.len() - 2..], ["EndLeadOut", "SetToolCorrection"], "{b:?}");
    }
    // Read back and written.
    call(&mut s, "export", None, json!({}));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn x003_pierces_a_contour_without_a_lead_in_inside_its_cut_out() {
    let d = dir("bysoft_x003");
    let mut s = bystar_job(&d, false);
    call(&mut s, "leads", None, json!({"mode": "delete"}));
    set(&mut s, "X003=1");
    let b = &blocks(&bystar_program_text(&mut s, 1))[0];
    // BV68: rapid, BeginCutOut, then the pierce BV33 gives (Fine by default).
    let at = b.iter().position(|c| c == "MoveLineTo").unwrap();
    assert_eq!(b[at..at + 4], ["MoveLineTo", "BeginCutOut", "SetPiercingMode", "ExecPiercing"], "{b:?}");
    // With the table's pierce (B014): Normal on every pierce, as the CAM writes it.
    let all: Vec<String> = (0..=12).map(|q| format!("{q}=Normal")).collect();
    call(&mut s, "machine-switches", None, json!({"machine": "Demo ByStar Fiber laser", "set": "B014=1", "pierce": all.join(",")}));
    let ccp = bystar_program_text(&mut s, 1);
    let pierces: Vec<_> = ccp_commands(&ccp).into_iter().filter(|c| c.0 == "SetPiercingMode").collect();
    assert!(!pierces.is_empty() && pierces.iter().all(|c| attr(c, "Value") == Some("Normal")));
    assert_eq!(pierces.len(), ccp.matches("<BeginCutOut").count(), "every contour pierced once");
    call(&mut s, "export", None, json!({}));
    // Engraving next to cuts (the proof job's label): pierced in its cut-out too,
    // with B036 (2: Normal) uncompensated and Interior.
    let mut p = proof_job(&d.join("proof"));
    set(&mut p, "X003=1, B036=2");
    let ccp = bystar_program_text(&mut p, 1);
    let cmds = ccp_commands(&ccp);
    let k = cmds.iter().position(|c| c.0 == "SetOperationMode" && attr(c, "Value") == Some("2")).unwrap();
    let k = k + cmds[k..].iter().position(|c| c.0 == "MoveLineTo").unwrap();
    let names: Vec<&str> = cmds[k..k + 4].iter().map(|c| c.0.as_str()).collect();
    assert_eq!(names[..], ["MoveLineTo", "BeginCutOut", "SetPiercingMode", "ExecPiercing"], "{:?}", &cmds[k..k + 6]);
    assert_eq!((attr(&cmds[k + 1], "Contour"), attr(&cmds[k + 2], "Value")), (Some("Interior"), Some("Normal")));
    call(&mut p, "export", None, json!({}));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn x004_turns_compensation_on_inside_the_lead_in_and_off_at_each_subprograms_start() {
    let d = dir("bysoft_x004");
    let mut s = bystar_job(&d, false);
    set(&mut s, "X004=1");
    let ccp = bystar_program_text(&mut s, 1);
    // BV68: each subprogram starts mode, compensation off.
    for part in ccp.split("<SubProgram ").skip(1) {
        let c = ccp_commands(part);
        assert_eq!((c[0].0.as_str(), c[1].0.as_str(), attr(&c[1], "Value")), ("SetOperationMode", "SetToolCorrection", Some("None")), "{part}");
    }
    let cmds = ccp_commands(&ccp);
    let (mut two, mut one) = (0, 0);
    for (i, c) in cmds.iter().enumerate().filter(|c| c.1 .0 == "BeginLeadIn") {
        let end = i + cmds[i..].iter().position(|c| c.0 == "EndLeadIn").unwrap();
        let moves: Vec<usize> = (i..end).filter(|&k| matches!(cmds[k].0.as_str(), "LineTo" | "ArcTo")).collect();
        let _ = c;
        if moves.len() >= 2 {
            // Before the lead-in's last move.
            assert_eq!(cmds[moves[moves.len() - 1] - 1].0, "SetToolCorrection", "{:?}", &cmds[i..=end]);
            two += 1;
        } else {
            // Right after BeginCutOut.
            assert_eq!((cmds[end + 1].0.as_str(), cmds[end + 2].0.as_str()), ("BeginCutOut", "SetToolCorrection"), "{:?}", &cmds[i..end + 3]);
            one += 1;
        }
        // Never switched on before the rapid.
        assert!(!(cmds[i - 2].0 == "SetToolCorrection" && attr(&cmds[i - 2], "Value") != Some("None")), "{:?}", &cmds[i - 3..=i]);
    }
    assert!(two + one > 0, "{ccp}");
    call(&mut s, "export", None, json!({}));
    // A one-move lead-in too (inclined line leads).
    call(&mut s, "leads", None, json!({"mode": "auto", "type": "line"}));
    let cmds = ccp_commands(&bystar_program_text(&mut s, 1));
    let i = cmds.iter().position(|c| c.0 == "EndLeadIn").unwrap();
    assert_eq!((cmds[i + 1].0.as_str(), cmds[i + 2].0.as_str()), ("BeginCutOut", "SetToolCorrection"));
    call(&mut s, "export", None, json!({}));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn x005_writes_the_operation_mode_on_every_contour() {
    let d = dir("bysoft_x005");
    let mut s = bystar_job(&d, false);
    let count = |s: &mut Session| {
        let ccp = bystar_program_text(s, 1);
        (ccp.matches("<SetOperationMode").count(), ccp.matches("<BeginCutOut").count())
    };
    let (modes, cuts) = count(&mut s);
    assert!(modes < cuts, "only on a change by default (BV51)");
    set(&mut s, "X005=1");
    let (modes, cuts) = count(&mut s);
    assert_eq!(modes, cuts, "BV69: one per contour");
    call(&mut s, "export", None, json!({}));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn x006_writes_part_positions_and_turns_exactly() {
    let d = dir("bysoft_x006");
    let mut s = form_job(&d);
    let calls = |s: &mut Session| -> Vec<(String, String, String)> {
        ccp_commands(&bystar_program_text(s, 1)).iter().filter(|c| c.0 == "CallSubProgram").map(|c| (attr(c, "X").unwrap().into(), attr(c, "Y").unwrap().into(), attr(c, "Rotation").unwrap().into())).collect()
    };
    // Rounded by default: x.5 toward zero (BV52).
    assert!(calls(&mut s).iter().any(|c| c.0 == "50" && c.1 == "20"), "{:?}", calls(&mut s));
    set(&mut s, "X006=1");
    let c = calls(&mut s);
    assert!(c.iter().any(|c| c.0 == "50.5" && c.1 == "20.5"), "BV70: {c:?}");
    call(&mut s, "export", None, json!({}));
    // A turn that isn't whole degrees stays in the call (an opened CAM nest at 13.5°).
    let files: Vec<(String, String)> = demo_bvc(DemoBvc::default()).into_iter().map(|(n, t)| (n, t.replace("Rotation=\"13\"", "Rotation=\"13.5\""))).collect();
    let bvc = write_bvc(&d, "DEMO-JOB.bvc", &files);
    let mut s = Session::new(Ctx::builtin());
    s.ctx.cfg = bystar_settings(&d, true);
    call(&mut s, "import-program", Some(d.join("DEMO-TURN.snjob").to_str().unwrap()), json!({"path": bvc}));
    set(&mut s, "X006=1");
    let c = calls(&mut s);
    assert!(c.iter().any(|c| c.2 == "13.5"), "{c:?}");
    call(&mut s, "export", None, json!({}));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn x007_names_an_inch_program_english_as_the_machine_does() {
    let d = dir("bysoft_x007");
    let mut s = bystar_job(&d, false);
    inch(&mut s, &d);
    assert!(bystar_program_text(&mut s, 1).contains("MeasurementSystem=\"Imperial\""));
    set(&mut s, "X007=1");
    let ccp = bystar_program_text(&mut s, 1);
    assert!(ccp.contains("MeasurementSystem=\"English\" Thickness=\"0.118110236\""), "BV70: {}", &ccp[..300]);
    call(&mut s, "export", None, json!({}));
    // A metric program (no inch post library, B001 off) stays Metric.
    call(&mut s, "machine-switches", None, json!({"machine": "Demo ByStar Fiber laser", "cf1": ""}));
    set(&mut s, "B001=0");
    assert!(bystar_program_text(&mut s, 1).contains("MeasurementSystem=\"Metric\""));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn x008_writes_the_full_job_file_and_the_read_back_checks_it() {
    let d = dir("bysoft_x008");
    let mut s = form_job(&d);
    set(&mut s, "X008=1");
    call(&mut s, "export", None, json!({}));
    let info = std::fs::read_to_string(bystar_dir(&d, "DEMO-FORM").join("JobInfo.xml")).unwrap();
    // BV71: the part's size and shape, the sheet's margins, the remnant sheet.
    assert!(info.contains("<PartInfo Id=\"1\" Name=\"DEMO-001\" Quantity=\"3\" SizeX=\"200\" SizeY=\"100\">"), "{info}");
    assert!(info.contains("<Geometries>") && info.contains("<Vertex PositionX=\"0\" PositionY=\"0\" />") && info.contains("Bulge=\""), "{info}");
    assert!(info.contains("MarginLeft=\"") && info.contains("FormatType=\"Sheet\""), "{info}");
    assert!(info.contains("<ResidualSheetInfo SizeX=\"") && info.contains("SizeY=\"500\" Bottom=\"0\" Left=\"600."), "{info}");
    // The reader takes it back: sizes, shapes (holes as arcs), margins and remnants in mm.
    let bvc = std::fs::read(bystar_dir(&d, "DEMO-FORM").join("DEMOFORMX3.bvc")).unwrap();
    let back = crate::bvc::read(&bvc).unwrap();
    let p1 = back.job.parts.iter().find(|p| p.name == "DEMO-001").unwrap();
    assert_eq!((p1.size, p1.shapes.len()), (Some([200.0, 100.0]), 4), "outline, two holes and the slot");
    // The remnant the job stores: past the remnant line's cut (half a kerf).
    let r = &back.job.plans[0].remnants;
    assert!(r.len() == 1 && (r[0][0] - 600.0).abs() < 0.5 && (r[0][0] + r[0][2] - 1000.0).abs() < 1e-6 && r[0][1] == 0.0 && r[0][3] == 500.0, "{r:?}");
    // Inch: the job file in the program's unit, named as the program is.
    inch(&mut s, &d);
    set(&mut s, "X007=1");
    call(&mut s, "export", None, json!({}));
    let info = std::fs::read_to_string(bystar_dir(&d, "DEMO-FORM").join("JobInfo.xml")).unwrap();
    assert!(info.contains("Thickness=\"0.118110236\" MeasurementSystem=\"English\"") && info.contains("SizeX=\"7.874015748\""), "{info}");
    // A job file that doesn't match the plan is refused by the read-back:
    // a part's size, a contour's shape, a remnant sheet.
    let edits = [
        (|t: String| t.replacen("SizeX=\"7.874015748\" SizeY=\"3.937007874\"", "SizeX=\"7.9\" SizeY=\"3.937007874\"", 1)) as fn(String) -> String,
        |t: String| t.replacen("<Vertex PositionX=\"0\" PositionY=\"0\" />", "<Vertex PositionX=\"0.1\" PositionY=\"0.1\" />", 1),
        |t: String| t.replacen("<ResidualSheetInfo SizeX=\"", "<ResidualSheetInfo SizeX=\"1", 1),
    ];
    for (k, edit) in edits.into_iter().enumerate() {
        crate::bystar::EDIT_INFO.with(|e| e.set(Some(edit)));
        let r = s.call("export", None, &json!({}));
        crate::bystar::EDIT_INFO.with(|e| e.set(None));
        let e = r.err().unwrap_or_else(|| panic!("edit {k} wasn't refused"));
        assert!(e.contains("wouldn't cut what's planned") && e.contains("job file"), "edit {k}: {e}");
    }
    call(&mut s, "export", None, json!({}));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn x009_puts_a_preview_picture_of_each_nest_in_the_bvc() {
    let d = dir("bysoft_x009");
    let mut s = form_job(&d);
    set(&mut s, "X009=1");
    call(&mut s, "export", None, json!({}));
    let folder = bystar_dir(&d, "DEMO-FORM");
    let bvc = std::fs::read(folder.join("DEMOFORMX1.bvc")).unwrap();
    let entries = crate::progimport::read_zip(&bvc).unwrap();
    let names: Vec<&str> = entries.iter().map(|e| e.0.as_str()).collect();
    // BV72: cut program, job file, picture (BV21's order).
    assert_eq!(names, ["DEMOFORMX1.ccp", "JobInfo.xml", "DEMOFORMX1.png"]);
    let png = image::load_from_memory(&entries[2].1).unwrap();
    assert_eq!((png.width(), png.height()), (500, 250));
    // The loose picture beside it (B010 off), and the reader still opens it.
    assert!(folder.join("DEMOFORMX1.png").is_file());
    assert!(crate::bvc::read(&bvc).is_ok());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn every_switch_of_ours_on_together_still_reads_back_on_the_awkward_jobs() {
    // Inch, joints, marks next to cuts, a remnant cut-off, x.5 positions, a
    // nest run twice, contours with and without lead-ins: all of this app's
    // switches on, as a shop matching the CAM's form would set them.
    let d = dir("bysoft_all_on");
    let all = "X002=1, X003=1, X004=1, X005=1, X006=1, X007=1, X008=1, X009=1, B036=2, B041=1, B039=2";
    for (name, mut s, job) in golden_cases(&d) {
        set(&mut s, all);
        let files = bystar_files_written(&mut s, &d.join(name), job);
        assert!(files.iter().any(|f| f.0.ends_with(".png")), "{name}");
    }
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn the_comparison_runs_on_a_made_up_cam_nest_and_names_nothing() {
    let d = dir("bysoft_compare_demo");
    let nests = d.join("nests");
    std::fs::create_dir_all(&nests).unwrap();
    write_bvc(&nests, "DEMO-JOB.bvc", &demo_bvc(DemoBvc::default()));
    let values = d.join("values.json");
    std::fs::write(&values, r#"{"set": "X002=1, X003=1, X004=1, X006=1, B036=2, B039=2, B014=1", "pierce": "0=Normal,1=Normal,2=Normal,3=Normal,4=Normal,5=Normal,6=Normal,7=Normal,8=Normal,9=Normal,10=Normal,11=Normal,12=Normal"}"#).unwrap();
    let out = compare::run(&nests, &values);
    assert!(out.contains("Nests in the CAM's files: 2") && out.contains("Differing commands by type"), "{out}");
    assert!(!out.contains("DEMO"), "no job, part or file names: {out}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn the_program_unit_is_picked_on_the_machine_and_its_name_follows() {
    // BV70: inch without a post library file; B001 (the unit's name) follows.
    let d = dir("bysoft_unit");
    let mut s = bystar_job(&d, false);
    let r = call(&mut s, "machine-switches", None, json!({"machine": "Demo ByStar Fiber laser", "unit": "inch"}));
    assert_eq!((r["data"]["unit"].as_str(), r["data"]["unit_from"].as_str()), (Some("inch"), Some("set")), "{r}");
    let b001 = r["data"]["switches"].as_array().unwrap().iter().find(|v| v["switch"] == "B001").unwrap().clone();
    assert_eq!(b001["value"], "1");
    let ccp = bystar_program_text(&mut s, 1);
    assert!(ccp.contains("MeasurementSystem=\"Imperial\" Thickness=\"0.118110236\""), "{}", &ccp[..300]);
    call(&mut s, "export", None, json!({}));
    // The settings page shows each switch in its group, in shop words.
    let x003 = r["data"]["switches"].as_array().unwrap().iter().find(|v| v["switch"] == "X003").unwrap().clone();
    assert_eq!((x003["group"].as_str(), x003["cam_form"].as_bool()), (Some("Piercing"), Some(true)), "{x003}");
    // Back to mm, then to the post library's (none: mm), B001 off again.
    let r = call(&mut s, "machine-switches", None, json!({"machine": "Demo ByStar Fiber laser", "unit": "mm"}));
    assert_eq!(r["data"]["unit"], "mm");
    assert!(bystar_program_text(&mut s, 1).contains("MeasurementSystem=\"Metric\""));
    let r = call(&mut s, "machine-switches", None, json!({"machine": "Demo ByStar Fiber laser", "unit": "file"}));
    assert_eq!((r["data"]["unit"].as_str(), r["data"]["unit_from"].as_str()), (Some("mm"), Some("default")), "{r}");
    let b001 = r["data"]["switches"].as_array().unwrap().iter().find(|v| v["switch"] == "B001").unwrap().clone();
    assert_eq!((b001["value"].as_str(), b001["from"].as_str()), (Some("0"), Some("default")));
    assert!(err(&mut s, "machine-switches", json!({"machine": "Demo ByStar Fiber laser", "unit": "furlongs"})).contains("mm, inch, or file"));
    // Settings saved before have no unit and load as they were.
    let old: crate::bystar::MachinePost = serde_json::from_str(r#"{"set": {"B015": "1"}}"#).unwrap();
    assert_eq!(old.unit, None);
    assert_eq!(serde_json::to_string(&old).unwrap(), r#"{"set":{"B015":"1"}}"#);
    let _ = std::fs::remove_dir_all(&d);
}

/// A 3000 × 1000 × 3 mm job on the made-up ByStar Fiber laser, four
/// 400 × 400 plates nested, the leftover cut into eight 500 × 500 standard
/// drops (cross cuts stopping on the strip cut).
fn drops_job(d: &Path) -> Session {
    let mut ctx = stock_ctx(d);
    ctx.cfg = crate::settings::AppSettings { stock: ctx.cfg.stock.clone(), ..bystar_settings(d, true) };
    ctx.cfg.stock.line.shape = crate::remline::LineShape::VerticalStrip;
    ctx.cfg.stock.drops = crate::engine::drops::DropSettings { on: true, sizes: vec![[1000.0, 500.0], [500.0, 500.0]], keep_odd: false };
    let mut s = Session::new(ctx);
    call(&mut s, "new", Some(d.join("DEMO-DROPS.snjob").to_str().unwrap()), json!({"w": 3000, "h": 1000, "thickness": 3}));
    call(&mut s, "add-shape", None, json!({"shape": "rectangle", "w": 400, "h": 400, "qty": 4, "name": "DEMO-PLATE"}));
    call(&mut s, "nest", None, json!({"stock": "off"}));
    let list = call(&mut s, "remnant", None, json!({"mode": "drops"}));
    let ways = list["data"].as_array().unwrap().clone();
    let stacked = ways.iter().find(|w| w["summary"].as_str().unwrap().starts_with("8 × 500 × 500")).unwrap_or_else(|| panic!("{list}"));
    call(&mut s, "remnant", None, json!({"mode": "drops", "layout": stacked["layout"].as_u64().unwrap()}));
    s
}

#[test]
fn x008_lists_each_remnant_the_job_stores_when_the_leftover_is_cut_into_drops() {
    // S1: standard drops (cross cuts ending on the strip cut) leave eight
    // remnants; the job file lists those eight, as the stock would store them.
    let d = dir("bysoft_x008_drops");
    let mut s = drops_job(&d);
    let st = call(&mut s, "stock-book", None, json!({"mode": "status"}));
    let rows = st["data"]["would_store_remnants"].as_array().unwrap().clone();
    assert_eq!(rows.len(), 8, "{st}");
    set(&mut s, "X008=1, B021=1");
    call(&mut s, "export", None, json!({}));
    let bvc = std::fs::read(bystar_dir(&d, "DEMO-DROPS").join("DEMODROPSX1.bvc")).unwrap();
    let back = crate::bvc::read(&bvc).unwrap();
    let mut got: Vec<[f64; 2]> = back.job.plans[0].remnants.iter().map(|r| [(r[2] * 10.0).round() / 10.0, (r[3] * 10.0).round() / 10.0]).collect();
    let mut want: Vec<[f64; 2]> = rows.iter().map(|r| [r["w"].as_f64().unwrap(), r["h"].as_f64().unwrap()]).collect();
    got.sort_by(|a, b| a.partial_cmp(b).unwrap());
    want.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(got, want, "one remnant sheet per remnant the job stores");
    // No two overlap.
    let r = &back.job.plans[0].remnants;
    for i in 0..r.len() {
        for j in i + 1..r.len() {
            let over = (r[i][0] + r[i][2]).min(r[j][0] + r[j][2]) - r[i][0].max(r[j][0]) > 0.01 && (r[i][1] + r[i][3]).min(r[j][1] + r[j][3]) - r[i][1].max(r[j][1]) > 0.01;
            assert!(!over, "{:?} overlaps {:?}", r[i], r[j]);
        }
    }
    // A job file that leaves one out is refused by the read-back.
    crate::bystar::EDIT_INFO.with(|e| e.set(Some(|t: String| {
        let a = t.find("<ResidualSheetInfo ").unwrap();
        let b = a + t[a..].find("/>").unwrap() + 2;
        format!("{}{}", &t[..a], &t[b..])
    })));
    let r = s.call("export", None, &json!({}));
    crate::bystar::EDIT_INFO.with(|e| e.set(None));
    let e = r.err().expect("a remnant left out of the job file wasn't refused");
    assert!(e.contains("wouldn't cut what's planned") && e.contains("remnant"), "{e}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn x008_refuses_a_remnant_that_isnt_a_rectangle_and_says_what_to_do() {
    // A remnant line with a step leaves an L-shaped remnant, which a
    // ResidualSheetInfo (a rectangle) can't describe.
    let d = dir("bysoft_x008_step");
    let mut s = form_job(&d);
    call(&mut s, "cut-line", None, json!({"sheet": 3, "points": "600,0;600,250;800,250;800,500"}));
    set(&mut s, "X008=1");
    let e = err(&mut s, "export", json!({}));
    assert!(e.contains("isn't one") && e.contains("turn X008 off"), "{e}");
    set(&mut s, "X008=0");
    call(&mut s, "export", None, json!({}));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn x008_refuses_a_contour_drawn_through_its_corners_in_another_order() {
    // S2: the rectangle's four corners, but joined across its diagonals.
    let d = dir("bysoft_x008_order");
    let mut s = form_job(&d);
    set(&mut s, "X008=1");
    call(&mut s, "export", None, json!({}));
    crate::bystar::EDIT_INFO.with(|e| e.set(Some(|t: String| {
        t.replacen(
            "<Vertex PositionX=\"0\" PositionY=\"0\" />\r\n            <Vertex PositionX=\"200\" PositionY=\"0\" />\r\n            <Vertex PositionX=\"200\" PositionY=\"100\" />\r\n            <Vertex PositionX=\"0\" PositionY=\"100\" />",
            "<Vertex PositionX=\"0\" PositionY=\"0\" />\r\n            <Vertex PositionX=\"200\" PositionY=\"100\" />\r\n            <Vertex PositionX=\"200\" PositionY=\"0\" />\r\n            <Vertex PositionX=\"0\" PositionY=\"100\" />",
            1,
        )
    })));
    let r = s.call("export", None, &json!({}));
    crate::bystar::EDIT_INFO.with(|e| e.set(None));
    let e = r.err().expect("a contour joined across its diagonals wasn't refused");
    assert!(e.contains("away from its shape"), "{e}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn x009_pictures_go_with_the_rest_of_the_jobs_old_programs() {
    // S3: a picture written with X009 is removed when the next write doesn't
    // make it (B010 on, or X009 off).
    let d = dir("bysoft_x009_stale");
    let mut s = form_job(&d);
    let folder = bystar_dir(&d, "DEMO-FORM");
    set(&mut s, "X009=1");
    call(&mut s, "export", None, json!({}));
    assert!(folder.join("DEMOFORMX1.png").is_file() && folder.join("DEMOFORMX3.png").is_file());
    set(&mut s, "B010=1");
    call(&mut s, "export", None, json!({}));
    assert!(!folder.join("DEMOFORMX1.png").exists(), "B010 keeps only the .bvc");
    set(&mut s, "B010=0, X009=0");
    call(&mut s, "export", None, json!({}));
    assert!(!folder.join("DEMOFORMX1.png").exists() && folder.join("DEMOFORMX1.ccp").is_file());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_cut_off_line_running_past_the_edge_senses_off_before_the_edge_not_at_its_end() {
    // BV38: standard drops' cuts run a little past the sheet; sensing goes
    // off where the line leaves the sheet, so the head never crosses the
    // edge with the sensor on, and the job is written.
    let d = dir("bysoft_drops_sensor");
    let mut s = drops_job(&d);
    call(&mut s, "export", None, json!({}));
    let ccp = std::fs::read_to_string(bystar_dir(&d, "DEMO-DROPS").join("DEMODROPSX1.ccp")).unwrap();
    let (_, c) = sub_commands(&ccp, "ResidualSheetCuts");
    let y = |x: &(String, Vec<(String, String)>)| attr(x, "Y").and_then(|v| v.parse::<f64>().ok());
    let mut checked = 0;
    for (i, _) in c.iter().enumerate().filter(|(_, x)| x.0 == "LineTo" && y(x).is_some_and(|v| v > 1000.0)) {
        // The move past the top edge comes after the sensor goes off on the edge.
        assert_eq!((c[i - 1].0.as_str(), attr(&c[i - 1], "Value")), ("SetSensingSensor", Some("Off")), "{:?}", &c[i - 3..=i]);
        assert_eq!(y(&c[i - 2]), Some(1000.0), "{:?}", &c[i - 3..=i]);
        checked += 1;
    }
    assert!(checked >= 3, "{ccp}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn drop_cuts_running_past_the_edge_with_b004_write_what_127_wrote() {
    // S4: with B004 = 1 the sensing-off point of a standard drop's cut
    // (ending 0.02 mm past the edge) is still on the sheet, as #127 wrote and
    // the read-back accepted: byte for byte the same files (golden, recorded
    // from #127's writer). With B004 = 0 that point fell past the edge (the
    // read-back refused the sheet); now it is written (sensing off on the edge).
    let d = dir("bysoft_drops_b004");
    let mut s = drops_job(&d);
    set(&mut s, "B004=1");
    let files = bystar_files_written(&mut s, &d, "DEMO-DROPS");
    let list: Vec<String> = files.iter().map(|f| f.0.clone()).collect();
    same_as_recorded("bystar-off-drops-b004-files.txt", &(list.join("\n") + "\n"));
    for (f, bytes) in &files {
        same_as_recorded(&format!("bystar-off-drops-b004-{}", f.replace('!', "--")), &String::from_utf8(bytes.clone()).unwrap());
    }
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn x008_refuses_a_remnant_with_a_narrow_step() {
    // S1: a remnant line with a 0.5 mm step leaves a stepped remnant whose
    // missing area is tiny; it still isn't a rectangle.
    let d = dir("bysoft_x008_narrow");
    let mut s = form_job(&d);
    call(&mut s, "cut-line", None, json!({"sheet": 3, "points": "600,0;600,250;600.5,250;600.5,500"}));
    set(&mut s, "X008=1");
    let e = err(&mut s, "export", json!({}));
    assert!(e.contains("isn't one") && e.contains("turn X008 off"), "{e}");
    set(&mut s, "X008=0");
    call(&mut s, "export", None, json!({}));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn b001_follows_the_picked_unit_whatever_the_files_and_other_settings_say() {
    // S5: with a unit picked, B001 comes from it when the post is loaded:
    // clearing the switches changed here ("Use the files' values for every
    // switch") or replacing the parameter file can't leave inch numbers named
    // Metric.
    let mut m = crate::bystar::MachinePost::default();
    m.set_unit(Some(true));
    m.set.clear();
    let p = crate::bystar::Post::load(&m);
    assert!(p.on("B001") && (p.unit - 25.4).abs() < 1e-9, "{:?}", p.raw("B001"));
    assert!(m.set_switch("B001", "0").unwrap_err().contains("follows the program unit"));
    let d = dir("bysoft_unit_files");
    let mut s = bystar_job(&d, false);
    let (inch_on, inch_off) = (d.join("DEMO-ON.ps1"), d.join("DEMO-OFF.ps1"));
    std::fs::write(&inch_on, "B001 1 ; inch\n").unwrap();
    std::fs::write(&inch_off, "B001 0 ; metric\n").unwrap();
    let ms = |s: &mut Session, a: Value| {
        let mut a = a;
        a["machine"] = json!("Demo ByStar Fiber laser");
        call(s, "machine-switches", None, a)
    };
    ms(&mut s, json!({"ps1": inch_on.to_str().unwrap()}));
    ms(&mut s, json!({"unit": "inch"}));
    ms(&mut s, json!({"ps1": inch_off.to_str().unwrap()}));
    let ccp = bystar_program_text(&mut s, 1);
    assert!(ccp.contains("MeasurementSystem=\"Imperial\" Thickness=\"0.118110236\""), "{}", &ccp[..300]);
    call(&mut s, "export", None, json!({}));
    // No unit picked: the files decide, as before (B001 from the parameter
    // file, the unit from the post library).
    ms(&mut s, json!({"unit": "file"}));
    let cf1 = d.join("DEMO-INCH.CF1");
    std::fs::write(&cf1, "A006 25.4 ; inch\n").unwrap();
    let r = ms(&mut s, json!({"ps1": inch_on.to_str().unwrap(), "cf1": cf1.to_str().unwrap()}));
    let b001 = r["data"]["switches"].as_array().unwrap().iter().find(|v| v["switch"] == "B001").unwrap().clone();
    assert_eq!((b001["value"].as_str(), b001["from"].as_str(), r["data"]["unit_from"].as_str()), (Some("1"), Some("file"), Some("file")), "{r}");
    assert!(bystar_program_text(&mut s, 1).contains("MeasurementSystem=\"Imperial\""));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn x008_refuses_a_bevelled_remnant_rather_than_list_its_rectangle() {
    // S6: on a 1000 × 1000 sheet the line 600,0 → 600,999 → 601,1000 leaves
    // a remnant with one corner cut off by a short diagonal; every corner of
    // it lies on its box's edges, but it isn't a rectangle.
    let d = dir("bysoft_x008_bevel");
    std::fs::create_dir_all(&d).unwrap();
    let mut s = Session::new(Ctx::builtin());
    s.ctx.cfg = bystar_settings(&d, true);
    call(&mut s, "new", Some(d.join("DEMO-BEVEL.snjob").to_str().unwrap()), json!({"w": 1000, "h": 1000, "material": "mild steel", "thickness": 3}));
    call(&mut s, "add-shape", None, json!({"shape": "rectangle", "w": 200, "h": 100, "qty": 1, "name": "DEMO-RECT"}));
    call(&mut s, "place", None, json!({"part": 1, "x": 50, "y": 50}));
    call(&mut s, "cut-line", None, json!({"sheet": 1, "points": "600,0;600,999;601,1000"}));
    set(&mut s, "X008=1");
    let e = err(&mut s, "export", json!({}));
    assert!(e.contains("isn't one") && e.contains("turn X008 off"), "{e}");
    // A straight line leaves a true rectangle, which is listed.
    call(&mut s, "cut-line", None, json!({"sheet": 1, "points": "600,0;600,1000"}));
    call(&mut s, "export", None, json!({}));
    let bvc = std::fs::read(bystar_dir(&d, "DEMO-BEVEL").join("DEMOBEVELX1.bvc")).unwrap();
    assert_eq!(crate::bvc::read(&bvc).unwrap().job.plans[0].remnants.len(), 1);
    let _ = std::fs::remove_dir_all(&d);
}
