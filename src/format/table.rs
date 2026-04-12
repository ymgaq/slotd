#[derive(Debug)]
pub(crate) struct TableColumn {
    pub(crate) header: String,
    pub(crate) width: usize,
    pub(crate) right_align: bool,
}

pub(crate) fn print_table<I, R>(columns: I, rows: R, noheader: bool)
where
    I: IntoIterator<Item = TableColumn>,
    R: IntoIterator<Item = Vec<String>>,
{
    let columns = columns.into_iter().collect::<Vec<_>>();
    if !noheader {
        let header = columns
            .iter()
            .map(|column| format_cell(&column.header, column.width, column.right_align))
            .collect::<Vec<_>>()
            .join(" | ");
        println!("{header}");
    }

    for row in rows {
        let line = row
            .iter()
            .enumerate()
            .map(|(index, value)| {
                let column = &columns[index];
                format_cell(value, column.width, column.right_align)
            })
            .collect::<Vec<_>>()
            .join(" | ");
        println!("{line}");
    }
}

fn truncate(value: &str, width: usize) -> String {
    if value.chars().count() <= width {
        return value.to_string();
    }
    let mut output = value
        .chars()
        .take(width.saturating_sub(1))
        .collect::<String>();
    output.push('-');
    output
}

pub(crate) fn format_cell(value: &str, width: usize, right_align: bool) -> String {
    let value = truncate(value, width);
    if right_align {
        format!("{value:>width$}")
    } else {
        format!("{value:<width$}")
    }
}
