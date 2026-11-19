use std::collections::{HashMap, HashSet};

pub type Coord = (isize, isize);

pub struct Universe {
    pub alive: HashSet<Coord>,
}

impl Universe {
    pub fn from_cells<I>(cells: I) -> Self
    where
        I: IntoIterator<Item = Coord>,
    {
        Self {
            alive: cells.into_iter().collect(),
        }
    }

    pub fn toggle_cell(&mut self, coord: Coord) {
        if !self.alive.remove(&coord) {
            self.alive.insert(coord);
        }
    }

    pub fn clear(&mut self) {
        self.alive.clear();
    }

    /// 提供只读视图供外部克隆快照
    pub fn alive_cells(&self) -> &HashSet<Coord> {
        &self.alive
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

        self.alive = neighbor_counts
            .into_iter()
            .filter(|&(pos, count)| {
                matches!((self.alive.contains(&pos), count), (true, 2 | 3) | (false, 3))
            })
            .map(|(pos, _)| pos)
            .collect();
    }
}