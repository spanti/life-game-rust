use std::collections::{HashMap, HashSet};
use std::sync::Arc;

pub type Coord = (isize, isize);

/// 携带元数据的只读快照
#[derive(Clone)]
pub struct Snapshot {
    pub cells: Arc<HashSet<Coord>>,
    pub generation: u64,
    pub paused: bool,
}

pub struct Universe {
    pub alive: HashSet<Coord>,
    generation: u64,
}

impl Universe {
    pub fn from_cells<I>(cells: I) -> Self
    where
        I: IntoIterator<Item = Coord>,
    {
        Self {
            alive: cells.into_iter().collect(),
            generation: 0,
        }
    }

    pub fn snapshot(&self, paused: bool) -> Snapshot {
        Snapshot {
            cells: Arc::new(self.alive.clone()),
            generation: self.generation,
            paused,
        }
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

        self.alive = neighbor_counts
            .into_iter()
            .filter(|&(pos, count)| {
                matches!((self.alive.contains(&pos), count), (true, 2 | 3) | (false, 3))
            })
            .map(|(pos, _)| pos)
            .collect();

        self.generation += 1;
    }
}