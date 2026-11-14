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
    /// Distance centre → tip, an expression in which `r` is the radius.
    Height(String),
}

#[derive(Debug, Clone)]
struct Options {
    circles: Vec<Target>,
    sketch: Option<Value>,
    up: UpSpec,
    tip: TipSpec,
    overhang: f64,
    flat: Option<String>,
}

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
        Some(Value::String(s)) => UpSpec::Explicit(geometry::parse_up(s).map_err(|e| Error::input("up", e))?),
        Some(_) => return Err(Error::input("up", "must be auto, +x, -x, +y, -y or an angle in degrees")),
    };
    let overhang = match params.get("overhang_angle") {
        None | Some(Value::Null) => 45.0,
        Some(v) => v
            .as_f64()
            .ok_or_else(|| Error::input("overhang_angle", "must be a number of degrees"))?,
    };
    if !(overhang > 0.0 && overhang < 90.0) {
        return Err(Error::input("overhang_angle", "must be between 0 and 90 degrees"));
    }
    let tip = match expr_field(params, "tip_height")? {
        None => TipSpec::Angle(overhang),
        Some(e) => TipSpec::Height(e),
    };
    Ok(Options {
        circles,
        sketch: params.get("sketch").cloned().filter(|s| !s.is_null()),
        up,
        tip,
        overhang,
        flat: expr_field(params, "flat_top")?,
    })
}

/// Replace the identifier `r` in an expression with `radius` (parenthesised).
pub fn substitute_r(expr: &str, radius: &str) -> String {
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
            // A digit directly before (as in "2r") would glue to the identifier: add a `*`.
            if ident == "r" {
                if out.chars().last().is_some_and(|p| p.is_ascii_digit() || p == '.') {
                    out.push('*');
                }
                out.push('(');
                out.push_str(radius);
                out.push(')');
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
fn resolve_targets(host: &mut dyn Host, opts: &Options) -> Result<(Option<Value>, Vec<String>, bool)> {
    let mut targets = opts.circles.clone();
    let from_selection = targets.is_empty();
    if from_selection {
        let sel = host.run("selection.get", &json!({"detail": true}))?;
        for it in sel.get("items").and_then(Value::as_array).into_iter().flatten() {
            let kind = it.get("kind").or_else(|| it.get("type")).and_then(Value::as_str);
            // `entity` may be missing (plugins run on a scratch copy): circles are checked later.
            let entity = it.get("entity").and_then(Value::as_str);
            if kind == Some("sketch_curve") && entity.is_none_or(|e| e == "circle") {
                if let Some(id) = it.get("id").and_then(Value::as_str) {
                    targets.push(Target { curve: id.into(), sketch: it.get("sketch").cloned() });
                }
            }
        }
        if targets.is_empty() {
            return Err(Error::input(
                "circles",
                "select one or more sketch circles first (or pass their ids in `circles`)",
            ));
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

/// Make sure the target sketch is the active one; returns whether we opened it.
fn activate(host: &mut dyn Host, sketch: &Option<Value>) -> Result<bool> {
    let doc = host.run("document.inspect", &json!({"timeline": false, "bodies": false, "measure": false}))?;
    let active = doc.get("active_sketch").filter(|v| !v.is_null()).cloned();
    match (sketch, active) {
        (None, Some(_)) => Ok(false),
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
                Ok(false)
            } else {
                Err(Error::input("sketch", "the circles are in another sketch than the one being edited; finish it first"))
            }
        }
        (Some(want), None) => {
            host.run("sketch.edit", &json!({"sketch": want}))?;
            Ok(true)
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
    let up = match up_spec {
        UpSpec::Explicit(v) => *v,
        UpSpec::Auto => {
            let plane = sk.get("plane");
            let (x, y) = (
                vec3(plane.and_then(|p| p.get("x_dir"))),
                vec3(plane.and_then(|p| p.get("y_dir"))),
            );
            match (x, y) {
                (Some(x), Some(y)) => geometry::auto_up(x, y).map_err(|e| Error::input("up", e))?,
                _ => return Err(Error::input("up", "the sketch reports no plane; set `up` explicitly")),
            }
        }
    };

    // `r` in the user's expressions is the circle's own radius expression, so the tip follows it.
    let r_expr = match &info.size_dim {
        Some((t, e)) if t == "diameter" => format!("({e}) / 2"),
        Some((_, e)) => e.clone(),
        None => format!("{} mm", num_fmt(info.radius)),
    };
    let tip_expr = match &opts.tip {
        TipSpec::Angle(_) => None,
        TipSpec::Height(e) => Some(substitute_r(e, &r_expr)),
    };
    let flat_expr = opts.flat.as_ref().map(|e| substitute_r(e, &r_expr));
    let tip = match &tip_expr {
        None => Tip::Angle(opts.overhang),
        Some(e) => Tip::Height(evaluate(host, "tip_height", e)?),
    };
    let flat = match &flat_expr {
        None => None,
        Some(e) => Some(evaluate(host, "flat_top", e)?),
    };
    let td = geometry::teardrop(info.center_at, info.radius, up, tip, flat).map_err(|e| {
        let field = if e.starts_with("flat_top") { "flat_top" } else if e.contains("tip") { "tip_height" } else { "overhang_angle" };
        Error::input(field, e)
    })?;
    let dof_before = sk.get("dof").and_then(Value::as_u64);

    // 1. The "up" axis: a construction line from the centre to the tip (or the cap's middle).
    let up_end = match td.cap {
        Some((a, b)) => Vec2::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0),
        None => td.tip,
    };
    let r = host.run(
        "sketch.line",
        &json!({"points": [info.center, pt(up_end)], "construction": true, "connect": false, "infer": false}),
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

    // 3. The flanks (and the cap).
    let (left, right, cap) = match td.cap {
        None => {
            let r = host.run(
                "sketch.line",
                &json!({"points": [arc_start, up_tip, arc_end], "infer": false}),
            )?;
            let c = created(&r, "curves");
            if c.len() != 2 {
                return Err(Error::command("sketch.line", format!("expected 2 flanks, got {c:?}")));
            }
            (c[0].clone(), c[1].clone(), None)
        }
        Some((a, b)) => {
            let r = host.run(
                "sketch.line",
                &json!({"points": [arc_start, pt(a), pt(b), arc_end], "infer": false}),
            )?;
            let c = created(&r, "curves");
            if c.len() != 3 {
                return Err(Error::command("sketch.line", format!("expected 2 flanks and a cap, got {c:?}")));
            }
            host.run("sketch.constraint.coincident", &json!({"a": up_tip, "b": {"midpoint": c[1]}}))?;
            host.run("sketch.constraint.perpendicular", &json!({"a": c[1], "b": up_line}))?;
            (c[0].clone(), c[2].clone(), Some(c[1].clone()))
        }
    };
    host.run("sketch.constraint.tangent", &json!({"a": left, "b": arc}))?;
    host.run("sketch.constraint.tangent", &json!({"a": right, "b": arc}))?;

    // 4. Swap the circle for the arc, keeping its size dimension (and so its parameter).
    host.run("sketch.delete", &json!({"entities": [id]}))?;
    let mut warnings: Vec<String> = Vec::new();
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

    // 6. Size the tip.
    let apex = match (&tip_expr, &flat_expr) {
        (None, _) => Some(format!("{} deg", num_fmt(td.apex_deg))),
        (Some(_), None) => None,
        // Flat top with a tip height: the tip is virtual, so hold the flank angle by expression.
        (Some(h), Some(_)) => Some(format!("2 * asin(({r_expr}) / ({h}))")),
    };
    if let Some(a) = &apex {
        host.run("sketch.dimension", &json!({"entities": [left, right], "type": "angle", "value": a}))?;
    }
    match (&tip_expr, &flat_expr) {
        (_, Some(f)) => {
            host.run("sketch.dimension", &json!({"entities": [up_line], "type": "length", "value": f}))?;
        }
        (Some(h), None) => {
            host.run("sketch.dimension", &json!({"entities": [up_line], "type": "length", "value": h}))?;
        }
        (None, None) => {}
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
    let mut out = json!({
        "circle": id,
        "arc": arc,
        "flanks": [left, right],
        "up_line": up_line,
        "up": [up.x, up.y],
        "radius": info.radius,
        "tip_height": td.tip_dist,
        "overhang_deg": 90.0 - td.apex_deg / 2.0,
        "dof": dof_after,
        "warnings": warnings,
    });
    if let Some(c) = cap {
        out["cap"] = json!(c);
        out["flat_top"] = json!(flat);
    }
    Ok(out)
}

pub fn run(host: &mut dyn Host, params: &Value) -> Result<Value> {
    let opts = parse_options(params)?;
    let (sketch, mut ids, from_selection) = resolve_targets(host, &opts)?;
    let opened = activate(host, &sketch)?;
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
        done.push(convert(host, &opts, &opts.up, id)?);
        host.log(&format!("teardrop: converted {id}"));
    }
    if opened {
        host.run("sketch.finish", &json!({}))?;
    }
    Ok(json!({"converted": done}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substitutes_r() {
        assert_eq!(substitute_r("r*sqrt(2)", "hole_r"), "(hole_r)*sqrt(2)");
        assert_eq!(substitute_r("1.2*r + 0.3 mm", "d1"), "1.2*(d1) + 0.3 mm");
        assert_eq!(substitute_r("2r", "d1"), "2*(d1)");
        assert_eq!(substitute_r("rr + r_1 + r", "x"), "rr + r_1 + (x)");
    }

    #[test]
    fn options_defaults() {
        let o = parse_options(&json!({})).unwrap();
        assert_eq!(o.up, UpSpec::Auto);
        assert_eq!(o.tip, TipSpec::Angle(45.0));
        assert!(o.flat.is_none() && o.circles.is_empty());
        let o = parse_options(&json!({"circles": ["c1", {"id": "c4", "sketch": 2}], "tip_height": "", "flat_top": 3, "up": "+x"})).unwrap();
        assert_eq!(o.circles.len(), 2);
        assert_eq!(o.tip, TipSpec::Angle(45.0));
        assert_eq!(o.flat.as_deref(), Some("3 mm"));
        assert_eq!(o.up, UpSpec::Explicit(Vec2::new(1.0, 0.0)));
        assert!(parse_options(&json!({"overhang_angle": 95})).is_err());
        assert!(parse_options(&json!({"up": "oben"})).is_err());
    }
}
