use crate::universe::Coord;
use std::{fmt::format, fs, path::Path, sync::Arc};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone)]
pub struct Pattern {
    pub name: String,
    pub cells: Arc<Vec<Coord>>,
}

impl Pattern {
    pub fn from_rle(default_name: &str, rle: &str) -> Result<Self, String> {
        let mut name = default_name.to_string();
        let mut body = String::new();
    
        for line in rle.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("#N") {
                // 提取 RLE 元数据中的图样名称
                let title = rest.trim();
                if !title.is_empty() {
                    name = title.to_string();
                }
            } else if trimmed.starts_with('#') || trimmed.starts_with('x') {
                // 跳过注释行与 "x = ..., y = ..., rule = ..." 头部声明
                continue;
            } else {
                body.push_str(trimmed);
            }
        }
    
        let mut raw_cells = Vec::new();
        let mut x: isize = 0;
        let mut y: isize = 0;
        let mut run_count: usize = 0;

        for ch in body.chars() {
            if ch.is_ascii_digit() {
                run_count = run_count * 10 + (ch as usize - '0' as usize);
            } else {
                let count = run_count.max(1);
                run_count = 0;
                match ch {
                    'b' => x += count as isize, // 死细胞（跳过）
                    'o' => {
                        // 活细胞
                        for i in 0..count {
                            raw_cells.push((x + i as isize, y));
                        }
                        x += count as isize;
                    }
                    '$' => {
                        // 换行（支持数字前缀，如 2$ 表示空两行）
                        y += count as isize;
                        x = 0;
                    }
                    '!' => break, // 结束符
                    _ if ch.is_whitespace() => {}
                    other => return Err(format!("RLE 包含非法字符: '{}'", other)),
                }
            }
        }

        let (min_x, max_x) = raw_cells
            .iter()
            .fold((isize::MAX, isize::MIN), |(mn, mx), &(cx, _)|{
                (mn.min(cx), mx.max(cx))
            });
    
        let (min_y, max_y) = raw_cells
            .iter()
            .fold((isize::MAX, isize::MIN), |(mn, mx), &(_, cy)|{
                (mn.min(cy), mx.max(cy))
            });

        let center_x = (min_x + max_x) / 2;
        let center_y = (min_y + max_y) / 2;
    
        let centered = raw_cells
            .into_iter()
            .map(|(cx, cy)| (cx - center_x, cy - center_y))
            .collect();

        Ok(Self {
            name,
            cells: Arc::new(centered)
        })
    }

    pub fn from_file(path: &Path) -> Result<Self, String> {
        let default_name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Imported RLE");

        let content = fs::read_to_string(path).map_err(|e| format!("读取文件失败: {}", e))?;
        Self::from_rle(default_name, &content)
    }
    

    /// 顺时针旋转 90 度：(x, y) -> (-y, x)
    pub fn rotate_cw(&mut self) {
        let rotated = self.cells.iter().map(|&(x, y)| (-y, x)).collect();
        self.cells = Arc::new(rotated);
    }

    /// 水平镜像翻转：(x, y) -> (-x, y)
    pub fn flip_horizontal(&mut self) {
        let flipped = self.cells.iter().map(|&(x, y)| (-x, y)).collect();
        self.cells = Arc::new(flipped);
    }

    /// 内置经典 RLE 预设图样库
    pub fn presets() -> Vec<Self> {
        let raw_presets = [
            ("Glider (滑翔机)", "x = 3, y = 3\nbo$2bo$3o!"),
            (
                "LWSS (轻量级飞船)",
                "x = 5, y = 4\nbo2bo$o$o3bo$4o!",
            ),
            (
                "Pulsar (脉冲星 - 周期3)",
                "x = 13, y = 13\n2b3o3b3o2b2$o4bobo4bo$o4bobo4bo$o4bobo4bo$2b3o3b3o2b2$2b3o3b3o2b$o4bobo4bo$o4bobo4bo$o4bobo4bo2$2b3o3b3o2b!",
            ),
            (
                "Pentadecathlon (十五连珠 - 周期15)",
                "x = 3, y = 10\nbob$bob$obo$bob$bob$bob$bob$obo$bob$bob!",
            ),
            (
                "Gosper Glider Gun (高斯帕滑翔机枪)",
                "x = 36, y = 9\n24bo$22bobo$12b2o6b2o12b2o$11bo3bo4b2o12b2o$2o8bo5bo3b2o$2o8bo3bob2o4bobo$10bo5bo7bo$11bo3bo$12b2o!",
            ),
        ];

        raw_presets
            .into_iter()
            .filter_map(|(name, rle)| Self::from_rle(name, rle).ok())
            .collect()
    }

    pub fn from_slice(name: &str, raw_cells: &[Coord]) -> Option<Self> {
        if raw_cells.is_empty() {
            return None;
        }

        let (min_x, max_x) = raw_cells
            .iter()
            .fold((isize::MAX, isize::MIN), |(mn, mx), &(cx, _)| {
                (mn.min(cx), mx.max(cx))
            });
        let (min_y, max_y) = raw_cells
            .iter()
            .fold((isize::MAX, isize::MIN), |(mn, mx), &(_, cy)| {
                (mn.min(cy), mx.max(cy))
            });

        let center_x = (min_x + max_x) / 2;
        let center_y = (min_y + max_y) / 2;

        let centered = raw_cells
            .iter()
            .map(|&(cx, cy)| (cx - center_x, cy - center_y))
            .collect();

        Some(Self {
            name: name.to_string(),
            cells: Arc::new(centered),
        })
    }

    pub fn to_rle(&self, rule_notation: &str) -> String {
        if self.cells.is_empty() {
            return format!("#N {}\nx = 0, y = 0, rule = {}\n!\n", self.name, rule_notation);
        }

        let (min_x, max_x) = self
            .cells
            .iter()
            .fold((isize::MAX, isize::MIN), |(mn, mx), &(x, _)| {
                (mn.min(x), mx.max(x))
            });
        let (min_y, max_y) = self
            .cells
            .iter()
            .fold((isize::MAX, isize::MIN), |(mn, mx), &(_, y)| {
                (mn.min(y), mx.max(y))
            });

        let width = (max_x - min_x + 1) as usize;
        let height = (max_y - min_y + 1) as usize;

        // 排序坐标以便按行扫描
        let mut sorted: Vec<Coord> = self
            .cells
            .iter()
            .map(|&(x, y)| (x - min_x, y - min_y))
            .collect();
        sorted.sort_unstable_by_key(|&(x, y)| (y, x));
        sorted.dedup();

        let mut output = format!(
            "#N {}\nx = {}, y = {}, rule = {}\n",
            self.name, width, height, rule_notation
        );
        let mut line_len = 0;

        let push_token = |out: &mut String, len: &mut usize, count: usize, ch: char| {
            let token = if count > 1 {
                format!("{}{}", count, ch)
            } else {
                ch.to_string()
            };

            if *len + token.len() > 70 {
                out.push('\n');
                *len = 0;
            }

            out.push_str(&token);
            *len += token.len();
        };

        let mut prev_y = 0;
        let mut idx = 0;

        while idx < sorted.len() {
            let y = sorted[idx].1;
            if y > prev_y {
                push_token(&mut output, &mut line_len, (y - prev_y) as usize, '$');
            }

            let mut cur_x = 0;
            while idx < sorted.len() && sorted[idx].1 == y {
                let x = sorted[idx].0;
                if x > cur_x {
                    push_token(&mut output, &mut line_len, (x - cur_x) as usize, 'b');
                }
                // 统计连续活细胞游程
                let mut run = 1;
                while idx + run < sorted.len()
                    && sorted[idx + run].1 == y
                    && sorted[idx + run].0 == x + run as isize
                {
                    run += 1;
                }
                push_token(&mut output, &mut line_len, run, 'o');
                cur_x = x + run as isize;
                idx += run;
            }
            prev_y = y;
        }

        output.push_str("!\n");
        output
    }

    pub fn save_to_timestamped_file(&self, rule_notation: &str) -> Result<String, String> {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let filename = format!("pattern_{}.rls", secs);
        let content = self.to_rle(rule_notation);
        fs::write(&filename, content).map_err(|e| format!("写入文件失败: {}", e))?;
        Ok(filename)
    }
}