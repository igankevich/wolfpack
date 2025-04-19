use std::borrow::Cow;
use std::io::Error;
use std::io::Write;

// TODO
pub trait ToRow<const N: usize> {
    fn to_row(&self) -> Row<'_, N>;
}

pub struct Row<'a, const N: usize>(pub [Cow<'a, str>; N]);

pub fn print_table<'a, I, T, const N: usize, W>(items: I, mut writer: W) -> Result<(), Error>
where
    I: IntoIterator<Item = &'a T>,
    T: ToRow<N> + 'a,
    W: Write,
{
    let rows = items.into_iter().map(ToRow::to_row).collect::<Vec<_>>();
    // Calculate columns' widths. Last column doesn't need width.
    let mut widths = [0_usize; N];
    for row in rows.iter() {
        for (i, width) in widths.iter_mut().enumerate().take(N - 1) {
            let num_chars = row.0[i].chars().count();
            if num_chars > *width {
                *width = num_chars;
            }
        }
    }
    // Add gaps.
    for width in widths.iter_mut().take(N - 1) {
        *width += COLUMN_GAP;
    }
    // Print.
    for row in rows.iter() {
        for (width, value) in widths.iter_mut().zip(&row.0).take(N - 1) {
            write!(writer, "{:width$}", value)?;
        }
        writeln!(writer, "{}", row.0[N - 1])?;
    }
    Ok(())
}

const COLUMN_GAP: usize = 2;
