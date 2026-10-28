use crate::GridSize;

pub const BASE_DPI: u32 = 96;
pub const DEFAULT_PADDING: u32 = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellSize {
    pub width: u32,
    pub height: u32,
}

impl CellSize {
    pub fn placeholder(dpi: u32) -> CellSize {
        CellSize {
            width: scale(9, dpi),
            height: scale(18, dpi),
        }
    }
}

pub fn scale(px: u32, dpi: u32) -> u32 {
    (px * dpi + BASE_DPI / 2) / BASE_DPI
}

pub fn grid_size(width: u32, height: u32, padding: u32, cell: CellSize) -> GridSize {
    let fit = |space: u32, cell: u32| -> u16 {
        let cells = space.saturating_sub(2 * padding) / cell.max(1);
        cells.clamp(1, u16::MAX as u32) as u16
    };
    GridSize {
        cols: fit(width, cell.width),
        rows: fit(height, cell.height),
    }
}

pub fn cell_at(x: i32, y: i32, padding: u32, cell: CellSize, grid: GridSize) -> (u16, u16) {
    let index = |pos: i32, size: u32, count: u16| -> u16 {
        let offset = (pos - padding as i32).max(0) as u32 / size.max(1);
        offset.min(u32::from(count.max(1)) - 1) as u16
    };
    (
        index(x, cell.width, grid.cols),
        index(y, cell.height, grid.rows),
    )
}

pub fn wrap(text: &str, columns: usize) -> Vec<String> {
    let columns = columns.max(1);
    let mut lines = Vec::new();
    for paragraph in text.lines() {
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            let needed =
                line.chars().count() + usize::from(!line.is_empty()) + word.chars().count();
            if !line.is_empty() && needed > columns {
                lines.push(std::mem::take(&mut line));
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
        lines.push(line);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointer_maps_to_clamped_cells() {
        let cell = CellSize {
            width: 10,
            height: 20,
        };
        let grid = GridSize { cols: 80, rows: 20 };
        assert_eq!(cell_at(12, 12, 12, cell, grid), (0, 0));
        assert_eq!(cell_at(35, 55, 12, cell, grid), (2, 2));
        assert_eq!(cell_at(-50, 5000, 12, cell, grid), (0, 19));
    }

    #[test]
    fn wrap_breaks_on_words() {
        assert_eq!(
            wrap("WSL is not installed. Run wsl --install", 16),
            ["WSL is not", "installed. Run", "wsl --install"]
        );
        assert_eq!(wrap("a\n\nb", 10), ["a", "", "b"]);
    }

    #[test]
    fn scale_rounds_to_device_pixels() {
        assert_eq!(scale(12, 96), 12);
        assert_eq!(scale(12, 144), 18);
        assert_eq!(scale(9, 120), 11);
    }

    #[test]
    fn grid_excludes_padding_on_both_sides() {
        let cell = CellSize {
            width: 10,
            height: 20,
        };
        assert_eq!(
            grid_size(824, 424, 12, cell),
            GridSize { cols: 80, rows: 20 }
        );
    }

    #[test]
    fn grid_never_collapses_to_zero() {
        let cell = CellSize {
            width: 10,
            height: 20,
        };
        assert_eq!(grid_size(0, 0, 12, cell), GridSize { cols: 1, rows: 1 });
    }
}
