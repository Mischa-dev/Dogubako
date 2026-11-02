//! Spreadsheet reading via calamine.

use crate::util::read_file;
use calamine::{Data, DataType, Reader, Sheets};
use dogubako_core::prelude::*;
use std::io::Cursor;

pub type Book = Sheets<Cursor<Vec<u8>>>;

pub fn open(f: &InputFile) -> ToolResult<Book> {
    let bytes = read_file(f)?;
    let c = Cursor::new(bytes);
    let bad = |e: String| {
        ToolError::invalid("file", format!("cannot read spreadsheet '{}': {e}", f.name))
    };
    Ok(match f.extension().as_str() {
        "xlsx" | "xlsm" | "xlam" => Sheets::Xlsx(
            calamine::open_workbook_from_rs(c)
                .map_err(|e: calamine::XlsxError| bad(e.to_string()))?,
        ),
        "xls" | "xla" => Sheets::Xls(
            calamine::open_workbook_from_rs(c)
                .map_err(|e: calamine::XlsError| bad(e.to_string()))?,
        ),
        "xlsb" => Sheets::Xlsb(
            calamine::open_workbook_from_rs(c)
                .map_err(|e: calamine::XlsbError| bad(e.to_string()))?,
        ),
        "ods" => Sheets::Ods(
            calamine::open_workbook_from_rs(c)
                .map_err(|e: calamine::OdsError| bad(e.to_string()))?,
        ),
        _ => calamine::open_workbook_auto_from_rs(c).map_err(|e| bad(e.to_string()))?,
    })
}

pub fn cell_text(d: &Data) -> String {
    match d {
        Data::DateTime(dt) => match dt.as_datetime() {
            Some(n) if n.time() == chrono::NaiveTime::MIN && !dt.is_duration() => {
                n.date().to_string()
            }
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

/// Most cells a sheet's occupied rectangle may span once filled in.
const MAX_SHEET_CELLS: u64 = 20_000_000;

/// All rows of a sheet, from its first to its last non-empty cell.
///
/// calamine's `worksheet_range` fills the rectangle between the outermost
/// cells before returning, so two cells at A1 and XFD1048576 would allocate
/// 17 billion cells. For XLSX and XLSB the cells are streamed instead and the
/// rectangle is checked before it is built. (XLS sheets are at most
/// 65536 x 256, and calamine caps ODS ranges itself.)
pub fn sheet_rows(book: &mut Book, name: &str) -> ToolResult<Vec<Vec<Data>>> {
    let bad = |e: String| ToolError::invalid("sheet", format!("cannot read sheet '{name}': {e}"));
    let mut cells: Vec<((u32, u32), Data)> = vec![];
    match book {
        Sheets::Xlsx(x) => {
            let mut r = x
                .worksheet_cells_reader(name)
                .map_err(|e| bad(e.to_string()))?;
            while let Some(c) = r.next_cell().map_err(|e| bad(e.to_string()))? {
                if !c.get_value().is_empty() {
                    cells.push((c.get_position(), Data::from(c.get_value().clone())));
                }
            }
        }
        Sheets::Xlsb(x) => {
            let mut r = x
                .worksheet_cells_reader(name)
                .map_err(|e| bad(e.to_string()))?;
            while let Some(c) = r.next_cell().map_err(|e| bad(e.to_string()))? {
                if !c.get_value().is_empty() {
                    cells.push((c.get_position(), Data::from(c.get_value().clone())));
                }
            }
        }
        _ => {
            let range = book.worksheet_range(name).map_err(|e| bad(e.to_string()))?;
            return Ok(range.rows().map(|r| r.to_vec()).collect());
        }
    }
    let Some(((r0, c0), (r1, c1))) =
        cells
            .iter()
            .fold(None::<((u32, u32), (u32, u32))>, |acc, ((r, c), _)| {
                Some(match acc {
                    None => ((*r, *c), (*r, *c)),
                    Some(((a, b), (y, z))) => ((a.min(*r), b.min(*c)), (y.max(*r), z.max(*c))),
                })
            })
    else {
        return Ok(vec![]);
    };
    let (h, w) = ((r1 - r0) as u64 + 1, (c1 - c0) as u64 + 1);
    if h * w > MAX_SHEET_CELLS {
        return Err(ToolError::limit(format!(
            "sheet '{name}' spans {h} rows x {w} columns ({} cells, max {MAX_SHEET_CELLS}); \
             remove stray cells far from the data",
            h * w
        )));
    }
    let mut rows = vec![vec![Data::Empty; w as usize]; h as usize];
    for ((r, c), v) in cells {
        rows[(r - r0) as usize][(c - c0) as usize] = v;
    }
    Ok(rows)
}
