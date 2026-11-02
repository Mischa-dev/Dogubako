//! Spreadsheet reading via calamine.

use crate::util::read_file;
use calamine::{Data, Reader, Sheets};
use dogubako_core::prelude::*;
use std::io::Cursor;

pub type Book = Sheets<Cursor<Vec<u8>>>;

pub fn open(f: &InputFile) -> ToolResult<Book> {
    let bytes = read_file(f)?;
    let c = Cursor::new(bytes);
    let bad = |e: String| ToolError::invalid("file", format!("cannot read spreadsheet '{}': {e}", f.name));
    Ok(match f.extension().as_str() {
        "xlsx" | "xlsm" | "xlam" => Sheets::Xlsx(calamine::open_workbook_from_rs(c).map_err(|e: calamine::XlsxError| bad(e.to_string()))?),
        "xls" | "xla" => Sheets::Xls(calamine::open_workbook_from_rs(c).map_err(|e: calamine::XlsError| bad(e.to_string()))?),
        "xlsb" => Sheets::Xlsb(calamine::open_workbook_from_rs(c).map_err(|e: calamine::XlsbError| bad(e.to_string()))?),
        "ods" => Sheets::Ods(calamine::open_workbook_from_rs(c).map_err(|e: calamine::OdsError| bad(e.to_string()))?),
        _ => calamine::open_workbook_auto_from_rs(c).map_err(|e| bad(e.to_string()))?,
    })
}

pub fn cell_text(d: &Data) -> String {
    match d {
        Data::DateTime(dt) => match dt.as_datetime() {
            Some(n) if n.time() == chrono::NaiveTime::MIN && !dt.is_duration() => n.date().to_string(),
            Some(n) if !dt.is_duration() => n.format("%Y-%m-%d %H:%M:%S").to_string(),
            _ => d.to_string(),
        },
        Data::Error(e) => format!("#{e:?}"),
        _ => d.to_string(),
    }
}

pub fn cell_json(d: &Data) -> Value {
    match d {
        Data::Int(i) => json!(i),
        Data::Float(f) if f.fract() == 0.0 && f.abs() < 9e15 => json!(*f as i64),
        Data::Float(f) => json!(f),
        Data::Bool(b) => json!(b),
        Data::Empty => Value::Null,
        _ => json!(cell_text(d)),
    }
}

/// All rows of a sheet as strings.
pub fn sheet_rows(book: &mut Book, name: &str) -> ToolResult<Vec<Vec<Data>>> {
    let range = book.worksheet_range(name).map_err(|e| ToolError::invalid("sheet", format!("cannot read sheet '{name}': {e}")))?;
    Ok(range.rows().map(|r| r.to_vec()).collect())
}
