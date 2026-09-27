//! Keep what a newer smix wrote when this one writes the ledger back.
//!
//! The ledger is one file per device, shared by every smix on the machine.
//! [`crate::Row::Unnamed`] keeps a whole row whose kind this binary does not
//! know; this keeps the fields it does not know inside the parts it does —
//! a row, the holder, the lease itself. Without it, a read-modify-write by
//! an older smix silently stripped them (10.1.0 took an 11.0 runner row's
//! `log` and `bundle` this way).
//!
//! What counts as "unknown" is measured, not listed: the file on disk is
//! read into a [`crate::Lease`] and serialized again, and whatever that
//! round trip loses is what this binary cannot see. A hand-kept list of
//! field names would be a second schema that drifts from the first.
//!
//! What is carried, and only when it is still the same thing:
//! - a row this binary left as it found it is written back as it was on
//!   disk, unknown fields and all. A row it changed or added is written as
//!   this binary understands it — its unknown fields described the old
//!   contents and there is nothing to say they fit the new ones;
//! - the holder, when it is still the holder that was read;
//! - the lease's own unknown fields, when it is still the same lease
//!   (same device, same `acquiredAt`). A lease that replaced it starts
//!   clean.

use serde_json::{Map, Value};

/// `on_disk` as read, `understood` the same file after a round trip
/// through this binary's types, `next` what this binary is about to write.
pub(crate) fn carry_forward(on_disk: &Value, understood: &Value, next: Value) -> Value {
    let (Some(disk), Some(seen), Value::Object(mut out)) =
        (on_disk.as_object(), understood.as_object(), next.clone())
    else {
        return next;
    };
    if !same_lease(seen, &out) {
        return next;
    }
    for (key, value) in disk {
        if !seen.contains_key(key) && !out.contains_key(key) {
            out.insert(key.clone(), value.clone());
        }
    }
    if let (Some(disk_holder), Some(seen_holder)) = (disk.get("holder"), seen.get("holder"))
        && out.get("holder") == Some(seen_holder)
    {
        out.insert("holder".into(), disk_holder.clone());
    }
    if let (
        Some(Value::Array(disk_rows)),
        Some(Value::Array(seen_rows)),
        Some(Value::Array(rows)),
    ) = (
        disk.get("resources"),
        seen.get("resources"),
        out.get_mut("resources"),
    ) {
        carry_rows(disk_rows, seen_rows, rows);
    }
    Value::Object(out)
}

fn same_lease(seen: &Map<String, Value>, next: &Map<String, Value>) -> bool {
    ["deviceId", "acquiredAt"]
        .iter()
        .all(|k| seen.get(*k).is_some() && seen.get(*k) == next.get(*k))
}

/// Each row of `rows` that equals a not-yet-used understood row is replaced
/// by that row as it was on disk. Rows are matched by content, not
/// position: a row dropped earlier in the list moves every later one.
fn carry_rows(disk_rows: &[Value], seen_rows: &[Value], rows: &mut [Value]) {
    if disk_rows.len() != seen_rows.len() {
        return;
    }
    let mut used = vec![false; seen_rows.len()];
    for row in rows.iter_mut() {
        if let Some(i) = (0..seen_rows.len()).find(|&i| !used[i] && seen_rows[i] == *row) {
            used[i] = true;
            *row = disk_rows[i].clone();
        }
    }
}
