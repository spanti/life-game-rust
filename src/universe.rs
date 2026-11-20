use std::collections::{HashMap, HashSet};
use std::sync::Arc;

pub type Coord = (isize, isize);

// TODO: seperate rule into rule.rs
/// Life-like 元胞自动机规则（使用 bit 0..=8 表示邻居数量）
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Rule {
    pub name: &'static str,
    pub notation: &'static str,
    born_mask: u16,
    survive_mask: u16,
}

impl Rule {
    /// 通过切片构造规则，例如 Rule::new("Conway", "B3/S23", &[3], &[2, 3])
    pub const fn new(
        name: &'static str,
        notation: &'static str,
        born: &[u8],
        survive: &[u8],
    ) -> Self {
        let mut born_mask = 0;
        let mut i = 0;
        while i < born.len() {
            born_mask |= 1 << born[i];
            i += 1;
        }

        let mut survive_mask = 0;
        let mut j = 0;
        while j < survive.len() {
            survive_mask |= 1 << survive[j];
            j += 1;
        }

        Self {
            name,
            notation,
            born_mask,
            survive_mask,
        }
    }

    #[inline]
    pub fn next_state(self, is_alive: bool, neighbors: u8) -> bool {
        let bit = 1u16 << neighbors;
        if is_alive {
            (self.survive_mask & bit) != 0
        } else {
            (self.born_mask & bit) != 0
        }
    }

    // 常用经典预设规则
    pub const CONWAY: Self = Self::new("Conway", "B3/S23", &[3], &[2, 3]);
    pub const HIGH_LIFE: Self = Self::new("HighLife", "B36/S23", &[3, 6], &[2, 3]);
    pub const SEEDS: Self = Self::new("Seeds", "B2/S", &[2], &[]);
    pub const DAY_AND_NIGHT: Self =
        Self::new("Day & Night", "B3678/S34678", &[3, 6, 7, 8], &[3, 4, 6, 7, 8]);
}

/// 携带元数据的只读快照
#[derive(Clone)]
pub struct Snapshot {
    pub cells: Arc<HashSet<Coord>>,
    pub generation: u64,
    pub paused: bool,
    pub rule: Rule,
    pub interval_ms: u64,
}

pub struct Universe {
    pub alive: HashSet<Coord>,
    generation: u64,
    rule: Rule,
}

impl Universe {
    pub fn from_cells<I>(cells: I) -> Self
    where
        I: IntoIterator<Item = Coord>,
    {
        Self {
            alive: cells.into_iter().collect(),
            generation: 0,
            rule: Rule::CONWAY,
        }
    }

    pub fn snapshot(&self, paused: bool, interval_ms: u64) -> Snapshot {
        Snapshot {
            cells: Arc::new(self.alive.clone()),
            generation: self.generation,
            paused,
            rule: self.rule,
            interval_ms
        }
    }

    pub fn restore(&mut self, snapshot: &Snapshot) {
        self.alive = (*snapshot.cells).clone();
        self.generation = snapshot.generation;
        self.rule = snapshot.rule;
    }
    
    pub fn set_rule(&mut self, rule: Rule) {
        self.rule = rule;
    }

    pub fn set_cell(&mut self, coord: Coord, is_alive: bool){
        if is_alive {
            self.alive.insert(coord);
        } else {
            self.alive.remove(&coord);
        }
    }

    pub fn clear(&mut self) {
        self.alive.clear();
        self.generation = 0;
    }

    fn neighbors((x, y): Coord) -> impl Iterator<Item = Coord> {
        (-1..=1)
            .flat_map(move |dy| (-1..=1).map(move |dx| (dx, dy)))
            .filter(|&(dx, dy)| dx != 0 || dy != 0)
            .map(move |(dx, dy)| (x + dx, y + dy))
    }

    pub fn tick(&mut self) {
        let mut neighbor_counts: HashMap<Coord, u8> = HashMap::new();

        for &cell in &self.alive {
            for neighbor in Self::neighbors(cell) {
                *neighbor_counts.entry(neighbor).or_insert(0) += 1;
            }
        }

        let rule = self.rule;
        let current_alive = &self.alive;

        self.alive = neighbor_counts
            .into_iter()
            .filter(|&(pos, count)| rule.next_state(current_alive.contains(&pos), count))
            .map(|(pos, _)| pos)
            .collect();

        self.generation += 1;
    }
}