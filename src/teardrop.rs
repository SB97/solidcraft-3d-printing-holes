//! Entry `teardrop`: replace selected sketch circles with constrained teardrop profiles.
//!
//! The result is ordinary native sketch geometry: an arc on the circle's centre, two
//! flanks tangent to it, a construction line from the centre showing where "up" is, and
//! dimensions that keep the shape following the circle's radius parameter.

use serde_json::{json, Value};

use crate::geometry::{self, Tip, Vec2};
use crate::host::{Error, Host, Result};

#[derive(Debug, Clone, PartialEq)]
enum UpSpec {
    Auto,
    Explicit(Vec2),
}

#[derive(Debug, Clone, PartialEq)]
enum TipSpec {
    Angle(f64),
    /// Distance centre → tip, an expression in which `r` is the radius and `d` the diameter.
    Height(String),
    /// Tip this many percent of the diameter above the circle. Written into the sketch as a
    /// readable expression that keeps the percentage, e.g. `hole_d / 2 + hole_d * 2.5 / 100`.
    PercentOfD(f64),
}

#[derive(Debug, Clone)]
struct Options {
    circles: Vec<Target>,
    sketch: Option<Value>,
    up: UpSpec,
    tip: TipSpec,
    /// Warn when `up: auto` meets a horizontal sketch and falls back to sketch +Y.
    warn_horizontal: bool,
    /// No ids given: convert every circle of the open sketch instead of the selection.
    all_in_sketch: bool,
}

/// Default tip: this many percent of the diameter above the circle (= 5 % of the radius).
/// Enough to bridge the sagging top of a printed hole without changing its shape much.
const DEFAULT_TIP_EXTRA_D: f64 = 2.5;

#[derive(Debug, Clone, PartialEq)]
struct Target {
    curve: String,
    sketch: Option<Value>,
}

/// An optional expression field: absent, null, "" and "auto" mean none; a JSON number is mm.
fn expr_field(params: &Value, name: &str) -> Result<Option<String>> {
    match params.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(n)) => Ok(Some(format!("{} mm", n))),
        Some(Value::String(s)) => {
            let t = s.trim();
            if t.is_empty() || t.eq_ignore_ascii_case("auto") {
                Ok(None)
            } else {
                Ok(Some(t.to_string()))
            }
        }
        Some(_) => Err(Error::input(name, "must be a number (mm) or an expression")),
    }
}

fn parse_target(v: &Value) -> Result<Target> {
    match v {
        Value::String(s) => Ok(Target { curve: s.clone(), sketch: None }),
        Value::Object(o) => {
            let curve = ["id", "curve"]
                .iter()
                .find_map(|k| o.get(*k).and_then(Value::as_str))
                .ok_or_else(|| Error::input("circles", "each item needs a curve id (\"c1\" or {id, sketch})"))?;
            Ok(Target { curve: curve.to_string(), sketch: o.get("sketch").cloned().filter(|s| !s.is_null()) })
        }
        _ => Err(Error::input("circles", "each item must be a curve id or {id, sketch}")),
    }
}

fn parse_options(params: &Value) -> Result<Options> {
    if !params.is_object() && !params.is_null() {
        return Err(Error::input("params", "must be an object"));
    }
    let circles = match params.get("circles") {
        None | Some(Value::Null) => vec![],
        Some(Value::Array(a)) => a.iter().map(parse_target).collect::<Result<_>>()?,
        Some(v @ Value::String(_)) | Some(v @ Value::Object(_)) => vec![parse_target(v)?],
        Some(_) => return Err(Error::input("circles", "must be a list of sketch circle ids")),
    };
    let up = match params.get("up") {
        None | Some(Value::Null) => UpSpec::Auto,
        Some(Value::Number(n)) => UpSpec::Explicit(
            geometry::parse_up(&n.to_string()).map_err(|e| Error::input("up", e))?,
        ),
        Some(Value::String(s)) if s.trim().is_empty() || s.trim().eq_ignore_ascii_case("auto") => UpSpec::Auto,
        // The form's dropdown: "angle" takes the direction from the separate `up_angle` field.
        Some(Value::String(s)) if s.trim().eq_ignore_ascii_case("angle") => {
            let a = match params.get("up_angle") {
                None | Some(Value::Null) => 90.0,
                Some(v) => v
                    .as_f64()
                    .ok_or_else(|| Error::input("up_angle", "must be a number of degrees"))?,
            };
            UpSpec::Explicit(geometry::parse_up(&a.to_string()).map_err(|e| Error::input("up_angle", e))?)
        }
        Some(Value::String(s)) => UpSpec::Explicit(geometry::parse_up(s).map_err(|e| Error::input("up", e))?),
        Some(_) => return Err(Error::input("up", "must be auto, +x, -x, +y, -y or an angle in degrees")),
    };
    // The tip, most specific first: an explicit height, an overhang angle (scripts only: the
    // classic 45° teardrop), else a tip a few percent of the radius above the circle.
    let overhang = match params.get("overhang_angle") {
        None | Some(Value::Null) => None,
        Some(v) => Some(
            v.as_f64()
                .ok_or_else(|| Error::input("overhang_angle", "must be a number of degrees"))?,
        ),
    };
    if let Some(a) = overhang {
        if !(a > 0.0 && a < 90.0) {
            return Err(Error::input("overhang_angle", "must be between 0 and 90 degrees"));
        }
    }
    // Tip above the circle as a share of the radius. The form field `tip_extra_d` is in percent
    // of the diameter; the older `tip_extra` (percent of the radius) is still read from scripts.
    let percent = |key: &str, of: &str| -> Result<Option<f64>> {
        match params.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(v) => {
                let p = v
                    .as_f64()
                    .ok_or_else(|| Error::input(key, format!("must be a number (percent of the {of})")))?;
                if !(p > 0.0) {
                    return Err(Error::input(key, format!("must be more than 0 % of the {of}")));
                }
                Ok(Some(p))
            }
        }
    };
    let extra_d = match (percent("tip_extra_d", "diameter")?, percent("tip_extra", "radius")?) {
        (Some(p), _) => p,
        (None, Some(p)) => p / 2.0,
        (None, None) => DEFAULT_TIP_EXTRA_D,
    };
    let tip = match (expr_field(params, "tip_height")?, overhang) {
        (Some(e), _) => TipSpec::Height(e),
        (None, Some(a)) => TipSpec::Angle(a),
        (None, None) => TipSpec::PercentOfD(extra_d),
    };
    let warn_horizontal = match params.get("warn_horizontal") {
        None | Some(Value::Null) => true,
        Some(v) => v
            .as_bool()
            .ok_or_else(|| Error::input("warn_horizontal", "must be true or false"))?,
    };
    let apply_to_all = match params.get("apply_to_all") {
        None | Some(Value::Null) => None,
        Some(v) => Some(v.as_bool().ok_or_else(|| Error::input("apply_to_all", "must be true or false"))?),
    };
    // `scope` is the older string form, still accepted from scripts.
    let scope = match params.get("scope") {
        None | Some(Value::Null) => false,
        Some(Value::String(s)) => match s.trim().to_ascii_lowercase().as_str() {
            "" | "selected" | "selection" => false,
            "all" | "all in sketch" => true,
            _ => return Err(Error::input("scope", "must be selected or all in sketch")),
        },
        Some(_) => return Err(Error::input("scope", "must be selected or all in sketch")),
    };
    let all_in_sketch = apply_to_all.unwrap_or(scope);
    Ok(Options {
        all_in_sketch,
        circles,
        sketch: params.get("sketch").cloned().filter(|s| !s.is_null()),
        up,
        tip,
        warn_horizontal,
    })
}

/// `e` as an operand: bare when it is a single name or number (`hole_r`, `23`, `8 mm`),
/// otherwise in parentheses.
fn operand(e: &str) -> String {
    let e = e.trim();
    if !e.is_empty() && !e.contains(|c: char| "+-*/^(),".contains(c)) {
        e.to_string()
    } else {
        format!("({e})")
    }
}

/// The default tip distance as an expression the user can read and edit: the percentage stays
/// visible instead of being folded into a factor. `size` is the circle's radius or diameter
/// dimension; without one the measured radius is used.
fn percent_tip_expr(size: Option<&(String, String)>, radius: f64, p: f64) -> String {
    let p = num_fmt(p);
    match size {
        Some((t, e)) if t == "radius" => {
            let r = operand(e);
            format!("{r} + 2 * {r} * {p} / 100")
        }
        Some((_, e)) => {
            let d = operand(e);
            format!("{d} / 2 + {d} * {p} / 100")
        }
        None => {
            let d = format!("{} mm", num_fmt(2.0 * radius));
            format!("{d} / 2 + {d} * {p} / 100")
        }
    }
}

/// Replace the identifiers `r` and `d` in an expression with `radius` and `diameter`
/// (parenthesised).
pub fn substitute_rd(expr: &str, radius: &str, diameter: &str) -> String {
    let mut out = String::with_capacity(expr.len() + radius.len());
    let chars: Vec<char> = expr.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let ident: String = chars[start..i].iter().collect();
            let value = match ident.as_str() {
                "r" => Some(radius),
                "d" => Some(diameter),
                _ => None,
            };
            if let Some(value) = value {
                // A digit directly before (as in "2r") would glue to the identifier: add a `*`.
                if out.chars().last().is_some_and(|p| p.is_ascii_digit() || p == '.') {
                    out.push('*');
                }
                out.push_str(&operand(value));
            } else {
                out.push_str(&ident);
            }
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

fn num_fmt(v: f64) -> String {
    let s = format!("{:.9}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}

fn pt(v: Vec2) -> Value {
    json!([v.x, v.y])
}

fn created(reply: &Value, kind: &str) -> Vec<String> {
    reply
        .pointer(&format!("/created/{kind}"))
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
        .unwrap_or_default()
}

fn first_created(reply: &Value, kind: &str, command: &str) -> Result<String> {
    created(reply, kind)
        .into_iter()
        .next()
        .ok_or_else(|| Error::command(command, format!("created no {kind}")))
}

fn vec3(v: Option<&Value>) -> Option<[f64; 3]> {
    let a = v?.as_array()?;
    Some([a.first()?.as_f64()?, a.get(1)?.as_f64()?, a.get(2)?.as_f64()?])
}

fn evaluate(host: &mut dyn Host, field: &str, expr: &str) -> Result<f64> {
    let r = host.run("expr.evaluate", &json!({"expression": expr, "kind": "length"}))?;
    if r.get("ok").and_then(Value::as_bool) != Some(true) {
        let why = r.get("error").and_then(Value::as_str).unwrap_or("does not evaluate");
        return Err(Error::input(field, format!("`{expr}`: {why}")));
    }
    r.get("value")
        .and_then(Value::as_f64)
        .ok_or_else(|| Error::input(field, format!("`{expr}` has no value")))
}

/// Find the sketch to work in and the circles to convert.
/// The sketch to work in, the curve ids, and whether they came from the selection.
/// Why the selection gave no circle, with what it did hold, so a user can tell what went wrong.
fn no_circle_selected(items: &[Value]) -> String {
    let what = if items.is_empty() {
        "nothing is selected".to_string()
    } else {
        let kinds: Vec<String> = items
            .iter()
            .take(5)
            .map(|it| {
                let kind = it.get("kind").or_else(|| it.get("type")).and_then(Value::as_str).unwrap_or("?");
                match it.get("entity").and_then(Value::as_str) {
                    Some(e) => format!("{kind} ({e})"),
                    None => kind.to_string(),
                }
            })
            .collect();
        format!("the selection holds {}: {}", items.len(), kinds.join(", "))
    };
    format!(
        "no sketch circle selected ({what}). Press Esc to end the active sketch tool, click the circles, then run; or tick `Apply to all circles in sketch`"
    )
}

fn resolve_targets(host: &mut dyn Host, opts: &Options) -> Result<(Option<Value>, Vec<String>, bool)> {
    let mut targets = opts.circles.clone();
    let from_selection = targets.is_empty();
    if from_selection {
        let sel = host.run("selection.get", &json!({"detail": true}))?;
        let items = sel.get("items").and_then(Value::as_array).cloned().unwrap_or_default();
        for it in &items {
            let kind = it.get("kind").or_else(|| it.get("type")).and_then(Value::as_str);
            // `entity` may be missing (plugins run on a scratch copy): circles are checked later.
            let entity = it.get("entity").and_then(Value::as_str);
            // A curve of another sketch than the one being edited comes as `external_sketch_curve`.
            let curve = matches!(kind, Some("sketch_curve") | Some("external_sketch_curve"));
            if curve && entity.is_none_or(|e| e == "circle") {
                if let Some(id) = it.get("id").or_else(|| it.get("curve")).and_then(Value::as_str) {
                    targets.push(Target { curve: id.into(), sketch: it.get("sketch").cloned() });
                }
            }
        }
        if targets.is_empty() {
            return Err(Error::input("circles", no_circle_selected(&items)));
        }
    }
    let mut sketch = opts.sketch.clone();
    for t in &targets {
        if let Some(s) = &t.sketch {
            match &sketch {
                None => sketch = Some(s.clone()),
                Some(cur) if cur != s => {
                    return Err(Error::input("circles", "all circles must be in the same sketch"))
                }
                _ => {}
            }
        }
    }
    let mut ids: Vec<String> = Vec::new();
    for t in targets {
        if !ids.contains(&t.curve) {
            ids.push(t.curve);
        }
    }
    Ok((sketch, ids, from_selection))
}

/// What `activate` changed, so `run` can put the editing state back afterwards.
struct Activation {
    /// We opened the target sketch, so we finish it again.
    opened: bool,
    /// Another sketch was being edited: we finished it, so we re-open it.
    reopen: Option<Value>,
}

/// Make sure the target sketch is the active one.
fn activate(host: &mut dyn Host, sketch: &Option<Value>) -> Result<Activation> {
    let doc = host.run("document.inspect", &json!({"timeline": false, "bodies": false, "measure": false}))?;
    let active = doc.get("active_sketch").filter(|v| !v.is_null()).cloned();
    match (sketch, active) {
        (None, Some(_)) => Ok(Activation { opened: false, reopen: None }),
        (None, None) => Err(Error::input("sketch", "open a sketch, or select circles in one")),
        (Some(want), Some(have)) => {
            let same = want == &have
                || doc
                    .get("sketches")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .any(|s| s.get("id") == Some(&have) && s.get("name") == Some(want));
            if same {
                Ok(Activation { opened: false, reopen: None })
            } else {
                // The circles are in another sketch than the one being edited: switch to it.
                host.run("sketch.finish", &json!({}))?;
                host.run("sketch.edit", &json!({"sketch": want}))?;
                Ok(Activation { opened: true, reopen: Some(have) })
            }
        }
        (Some(want), None) => {
            host.run("sketch.edit", &json!({"sketch": want}))?;
            Ok(Activation { opened: true, reopen: None })
        }
    }
}

struct CircleInfo {
    index: usize,
    center: String,
    center_at: Vec2,
    radius: f64,
    /// The radius or diameter dimension on the circle: (type, expression).
    size_dim: Option<(String, String)>,
    /// Other constraints on the circle, which are lost with it.
    dropped: Vec<String>,
}

fn circle_info(sketch: &Value, id: &str) -> Result<CircleInfo> {
    let curves = sketch.get("curves").and_then(Value::as_array).cloned().unwrap_or_default();
    let index = curves
        .iter()
        .position(|c| c.get("id").and_then(Value::as_str) == Some(id))
        .ok_or_else(|| Error::input("circles", format!("`{id}` is not a curve of the active sketch")))?;
    let c = &curves[index];
    if c.get("type").and_then(Value::as_str) != Some("circle") {
        return Err(Error::input("circles", format!("`{id}` is not a circle")));
    }
    if c.get("construction").and_then(Value::as_bool) == Some(true) {
        return Err(Error::input("circles", format!("`{id}` is a construction circle")));
    }
    let center = c.get("center").and_then(Value::as_str).unwrap_or_default().to_string();
    let at = c.get("center_at").and_then(Value::as_array);
    let center_at = Vec2::new(
        at.and_then(|a| a.first()).and_then(Value::as_f64).unwrap_or(0.0),
        at.and_then(|a| a.get(1)).and_then(Value::as_f64).unwrap_or(0.0),
    );
    let radius = c
        .get("radius")
        .and_then(Value::as_f64)
        .ok_or_else(|| Error::command("sketch.inspect", format!("circle `{id}` has no radius")))?;
    let mut size_dim = None;
    let mut dropped = Vec::new();
    for k in sketch.get("constraints").and_then(Value::as_array).into_iter().flatten() {
        let ty = k.get("type").and_then(Value::as_str).unwrap_or("");
        let on_curve = ["a", "b", "c", "l"]
            .iter()
            .any(|f| k.get(*f).and_then(Value::as_u64) == Some(index as u64));
        if !on_curve {
            continue;
        }
        if (ty == "radius" || ty == "diameter") && size_dim.is_none() {
            let expr = k
                .get("expression")
                .and_then(Value::as_str)
                .map(String::from)
                .unwrap_or_else(|| num_fmt(k.get("value").and_then(Value::as_f64).unwrap_or(radius)));
            size_dim = Some((ty.to_string(), expr));
        } else {
            let name = k.get("id").and_then(Value::as_str).unwrap_or("?");
            dropped.push(format!("{name} ({ty})"));
        }
    }
    if center.is_empty() {
        return Err(Error::command("sketch.inspect", format!("circle `{id}` has no centre point")));
    }
    Ok(CircleInfo { index, center, center_at, radius, size_dim, dropped })
}

fn convert(host: &mut dyn Host, opts: &Options, up_spec: &UpSpec, id: &str) -> Result<Value> {
    let sk = host.run("sketch.inspect", &json!({}))?;
    let info = circle_info(&sk, id)?;
    let _ = info.index;
    let mut warnings: Vec<String> = Vec::new();
    let up = match up_spec {
        UpSpec::Explicit(v) => *v,
        UpSpec::Auto => {
            let plane = sk.get("plane");
            let (x, y) = (
                vec3(plane.and_then(|p| p.get("x_dir"))),
                vec3(plane.and_then(|p| p.get("y_dir"))),
            );
            // The design's orientation need not be the print orientation, so a horizontal
            // sketch is no error: the tip goes to sketch +Y, with a warning one can turn off.
            match (x, y) {
                (Some(x), Some(y)) => geometry::auto_up(x, y).unwrap_or_else(|| {
                    if opts.warn_horizontal {
                        warnings.push(format!(
                            "`{id}`: the sketch is horizontal, so world +Z gives no direction; the tip \
                             points to sketch +Y. Set Up if the part prints in another orientation."
                        ));
                    }
                    Vec2::new(0.0, 1.0)
                }),
                _ => return Err(Error::input("up", "the sketch reports no plane; set Up to +x, -x, +y, -y or angle")),
            }
        }
    };

    // `r` and `d` in the user's expressions are the circle's own size expression, so the tip
    // follows it.
    let (r_expr, d_expr) = match &info.size_dim {
        Some((t, e)) if t == "diameter" => (format!("{} / 2", operand(e)), e.clone()),
        Some((_, e)) => (e.clone(), format!("2 * {}", operand(e))),
        None => (format!("{} mm", num_fmt(info.radius)), format!("{} mm", num_fmt(2.0 * info.radius))),
    };
    let tip_expr = match &opts.tip {
        TipSpec::Angle(_) => None,
        TipSpec::Height(e) => Some(substitute_rd(e, &r_expr, &d_expr)),
        TipSpec::PercentOfD(p) => Some(percent_tip_expr(info.size_dim.as_ref(), info.radius, *p)),
    };
    let tip = match (&opts.tip, &tip_expr) {
        (_, Some(e)) => Tip::Height(evaluate(host, "tip_height", e)?),
        (TipSpec::Angle(a), None) => Tip::Angle(*a),
        (_, None) => unreachable!("a height always has an expression"),
    };
    let td = geometry::teardrop(info.center_at, info.radius, up, tip).map_err(|e| {
        let field = if e.contains("tip") { "tip_height" } else { "overhang_angle" };
        Error::input(field, e)
    })?;
    let dof_before = sk.get("dof").and_then(Value::as_u64);

    // 1. The "up" axis: a construction line from the centre to the tip.
    let r = host.run(
        "sketch.line",
        &json!({"points": [info.center, pt(td.tip)], "construction": true, "connect": false, "infer": false}),
    )?;
    let up_line = first_created(&r, "curves", "sketch.line")?;
    let up_tip = format!("{up_line}.end");

    // 2. The arc around the bottom, counter-clockwise from the left tangent point to the right one.
    let r = host.run(
        "sketch.arc.center_point",
        &json!({"center": info.center, "start": pt(td.t_left), "end": pt(td.t_right), "connect": false}),
    )?;
    let arc = first_created(&r, "curves", "sketch.arc.center_point")?;
    let (arc_start, arc_end) = (format!("{arc}.start"), format!("{arc}.end"));

    // 3. The flanks.
    let r = host.run("sketch.line", &json!({"points": [arc_start, up_tip, arc_end], "infer": false}))?;
    let c = created(&r, "curves");
    if c.len() != 2 {
        return Err(Error::command("sketch.line", format!("expected 2 flanks, got {c:?}")));
    }
    let (left, right) = (c[0].clone(), c[1].clone());
    host.run("sketch.constraint.tangent", &json!({"a": left, "b": arc}))?;
    host.run("sketch.constraint.tangent", &json!({"a": right, "b": arc}))?;

    // 4. Swap the circle for the arc, keeping its size dimension (and so its parameter).
    host.run("sketch.delete", &json!({"entities": [id]}))?;
    match &info.size_dim {
        Some((ty, e)) => {
            host.run("sketch.dimension", &json!({"entities": [arc], "type": ty, "value": e}))?;
        }
        None => warnings.push(format!(
            "`{id}` had no radius dimension: the teardrop's radius is free, as the circle's was"
        )),
    }
    if !info.dropped.is_empty() {
        warnings.push(format!(
            "constraints on `{id}` that did not carry over: {}",
            info.dropped.join(", ")
        ));
    }

    // 5. Hold the up axis.
    if up.x == 0.0 {
        host.run("sketch.constraint.horizontal_vertical", &json!({"line": up_line, "mode": "vertical"}))?;
    } else if up.y == 0.0 {
        host.run("sketch.constraint.horizontal_vertical", &json!({"line": up_line, "mode": "horizontal"}))?;
    } else {
        // The angle between two lines is taken as an undirected line angle: a value of 180° or
        // more makes the solver swing the line over to the other side, so give it in [0, 180).
        // The line is already drawn in the right direction and stays there.
        let a = up.angle_deg() % 180.0;
        host.run(
            "sketch.dimension",
            &json!({"entities": [up_line, "x"], "type": "angle", "value": format!("{} deg", num_fmt(a))}),
        )?;
    }

    // 6. Size the tip: a length expression on the up-line, or the apex angle for an overhang angle.
    match &tip_expr {
        Some(h) => {
            host.run("sketch.dimension", &json!({"entities": [up_line], "type": "length", "value": h}))?;
        }
        None => {
            let a = format!("{} deg", num_fmt(td.apex_deg));
            host.run("sketch.dimension", &json!({"entities": [left, right], "type": "angle", "value": a}))?;
        }
    }

    // 7. Check the solve.
    let s = host.run("sketch.solve", &json!({}))?;
    let status = s.get("status").and_then(Value::as_str).unwrap_or("");
    let failing = s.get("failing").and_then(Value::as_array).map(|a| a.len()).unwrap_or(0);
    if status != "solved" || failing > 0 {
        return Err(Error::command("sketch.solve", format!("the teardrop for `{id}` does not solve: {s}")));
    }
    let dof_after = s.get("dof").and_then(Value::as_u64);
    if let (Some(b), Some(a)) = (dof_before, dof_after) {
        if a > b {
            warnings.push(format!("the sketch has {a} degrees of freedom, {b} before"));
        }
    }
    let out = json!({
        "circle": id,
        "arc": arc,
        "flanks": [left, right],
        "up_line": up_line,
        "up": [up.x, up.y],
        "radius": info.radius,
        "tip_height": td.tip_dist,
        "tip_expression": tip_expr,
        "overhang_deg": 90.0 - td.apex_deg / 2.0,
        "dof": dof_after,
        "warnings": warnings,
    });
    Ok(out)
}

pub fn run(host: &mut dyn Host, params: &Value) -> Result<Value> {
    let opts = parse_options(params)?;
    let all = opts.circles.is_empty() && opts.all_in_sketch;
    let (sketch, mut ids, from_selection) =
        if all { (opts.sketch.clone(), Vec::new(), false) } else { resolve_targets(host, &opts)? };
    let act = activate(host, &sketch)?;
    if all {
        let sk = host.run("sketch.inspect", &json!({}))?;
        ids = sk
            .get("curves")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|c| c.get("id").and_then(Value::as_str).map(String::from))
            .filter(|id| circle_info(&sk, id).is_ok())
            .collect();
        if ids.is_empty() {
            return Err(Error::input("apply_to_all", "the open sketch has no (non-construction) circle"));
        }
    }
    if from_selection {
        // Keep only the selected curves that are real circles; other selected curves are ignored.
        let sk = host.run("sketch.inspect", &json!({}))?;
        ids.retain(|id| circle_info(&sk, id).is_ok());
        if ids.is_empty() {
            return Err(Error::input("circles", "the selection holds no (non-construction) sketch circle"));
        }
    }
    let mut done = Vec::new();
    for id in &ids {
        let out = convert(host, &opts, &opts.up, id)?;
        for w in out["warnings"].as_array().into_iter().flatten().filter_map(Value::as_str) {
            host.warn(&format!("teardrop: {w}"));
        }
        host.log(&format!("teardrop: converted {id}"));
        done.push(out);
    }
    if act.opened {
        host.run("sketch.finish", &json!({}))?;
    }
    if let Some(prev) = act.reopen {
        host.run("sketch.edit", &json!({"sketch": prev}))?;
    }
    Ok(json!({"converted": done}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substitutes_r_and_d() {
        assert_eq!(substitute_rd("r*sqrt(2)", "hole_r", "D"), "hole_r*sqrt(2)");
        assert_eq!(substitute_rd("1.2*r + 0.3 mm", "d1", "D"), "1.2*d1 + 0.3 mm");
        assert_eq!(substitute_rd("2r", "d1", "D"), "2*d1");
        assert_eq!(substitute_rd("rr + r_1 + r", "x", "D"), "rr + r_1 + x");
        assert_eq!(substitute_rd("d/2 + 0.3 mm", "x", "hole_d"), "hole_d/2 + 0.3 mm");
        assert_eq!(substitute_rd("0.6d + dd + d1", "x", "D"), "0.6*D + dd + d1");
        // Compound values keep their parentheses.
        assert_eq!(substitute_rd("1.2*r", "23 / 2", "23"), "1.2*(23 / 2)");
        assert_eq!(substitute_rd("2*d", "x", "a+b"), "2*(a+b)");
    }

    #[test]
    fn percent_tip_is_readable() {
        let dia = ("diameter".to_string(), "23".to_string());
        assert_eq!(percent_tip_expr(Some(&dia), 11.5, 2.5), "23 / 2 + 23 * 2.5 / 100");
        let rad = ("radius".to_string(), "hole_r".to_string());
        assert_eq!(percent_tip_expr(Some(&rad), 4.0, 2.5), "hole_r + 2 * hole_r * 2.5 / 100");
        let rad = ("radius".to_string(), "hole_d / 2".to_string());
        assert_eq!(percent_tip_expr(Some(&rad), 4.0, 3.0), "(hole_d / 2) + 2 * (hole_d / 2) * 3 / 100");
        assert_eq!(percent_tip_expr(None, 4.0, 2.5), "8 mm / 2 + 8 mm * 2.5 / 100");
    }

    #[test]
    fn options_defaults() {
        let o = parse_options(&json!({})).unwrap();
        assert_eq!(o.up, UpSpec::Auto);
        assert_eq!(o.tip, TipSpec::PercentOfD(2.5));
        assert!(o.circles.is_empty() && o.warn_horizontal);
        let o = parse_options(&json!({"circles": ["c1", {"id": "c4", "sketch": 2}], "tip_height": "", "up": "+x"})).unwrap();
        assert_eq!(o.circles.len(), 2);
        assert_eq!(o.tip, TipSpec::PercentOfD(2.5));
        assert_eq!(o.up, UpSpec::Explicit(Vec2::new(1.0, 0.0)));
        assert!(parse_options(&json!({"overhang_angle": 95})).is_err());
        assert!(parse_options(&json!({"up": "oben"})).is_err());
    }

    #[test]
    fn tip_precedence() {
        // tip_extra_d in percent of the diameter.
        let o = parse_options(&json!({"tip_extra_d": 6})).unwrap();
        assert_eq!(o.tip, TipSpec::PercentOfD(6.0));
        assert!(parse_options(&json!({"tip_extra_d": 0})).is_err());
        assert!(parse_options(&json!({"tip_extra_d": "x"})).is_err());
        // The older tip_extra (percent of r) still works from scripts; tip_extra_d wins.
        let o = parse_options(&json!({"tip_extra": 12.5})).unwrap();
        assert_eq!(o.tip, TipSpec::PercentOfD(6.25));
        assert!(parse_options(&json!({"tip_extra": 0})).is_err());
        let o = parse_options(&json!({"tip_extra": 12.5, "tip_extra_d": 2.5})).unwrap();
        assert_eq!(o.tip, TipSpec::PercentOfD(2.5));
        // An overhang angle (scripts) beats tip_extra, an explicit height beats both.
        let o = parse_options(&json!({"tip_extra": 5, "overhang_angle": 45})).unwrap();
        assert_eq!(o.tip, TipSpec::Angle(45.0));
        let o = parse_options(&json!({"tip_extra": 5, "overhang_angle": 45, "tip_height": "r + 0.3 mm"})).unwrap();
        assert_eq!(o.tip, TipSpec::Height("r + 0.3 mm".into()));
    }

    /// What the generated form sends: every field, with its defaults.
    #[test]
    fn options_from_form() {
        let form = json!({"apply_to_all": false, "up": "auto", "up_angle": 90, "warn_horizontal": true, "tip_extra_d": 2.5, "tip_height": ""});
        let o = parse_options(&form).unwrap();
        assert_eq!(o.up, UpSpec::Auto);
        assert_eq!(o.tip, TipSpec::PercentOfD(2.5));
        assert!(o.circles.is_empty() && o.sketch.is_none() && o.warn_horizontal);
        assert!(!parse_options(&json!({"warn_horizontal": false})).unwrap().warn_horizontal);
        assert!(parse_options(&json!({"warn_horizontal": "no"})).is_err());
        let o = parse_options(&json!({"up": "angle", "up_angle": 180})).unwrap();
        assert_eq!(o.up, UpSpec::Explicit(geometry::parse_up("180").unwrap()));
        // up_angle only counts with up = angle.
        let o = parse_options(&json!({"up": "-y", "up_angle": 180})).unwrap();
        assert_eq!(o.up, UpSpec::Explicit(Vec2::new(0.0, -1.0)));
        assert!(parse_options(&json!({"up": "angle", "up_angle": "x"})).is_err());
        assert!(!parse_options(&json!({"scope": "selected"})).unwrap().all_in_sketch);
        assert!(parse_options(&json!({"scope": "all in sketch"})).unwrap().all_in_sketch);
        assert!(parse_options(&json!({"scope": "some"})).is_err());
        assert!(!parse_options(&json!({"apply_to_all": false})).unwrap().all_in_sketch);
        assert!(parse_options(&json!({"apply_to_all": true})).unwrap().all_in_sketch);
        assert!(parse_options(&json!({"apply_to_all": "yes"})).is_err());
        // The checkbox wins over the older `scope` string.
        assert!(!parse_options(&json!({"apply_to_all": false, "scope": "all"})).unwrap().all_in_sketch);
    }

    #[test]
    fn explains_empty_selection() {
        assert!(no_circle_selected(&[]).contains("nothing is selected"));
        let m = no_circle_selected(&[json!({"kind": "face"}), json!({"kind": "sketch_curve", "entity": "line"})]);
        assert!(m.contains("holds 2: face, sketch_curve (line)"), "{m}");
    }
}
