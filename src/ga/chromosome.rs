use std::collections::HashSet;

use rand::random;

#[derive(Debug)]
pub struct Chromosome {
    positions: Vec<u16>,
    conflicts: Vec<u16>,
    conflicts_sum: u16,
    fitness: f32,
}

impl Chromosome {
    pub fn new(positions: Vec<u16>) -> Self {
        let conflicts = count_conflicts(&positions);
        let conflicts_sum = conflicts.iter().sum::<u16>() / 2;
        log::debug!("chromosome conflicts sum: {conflicts_sum}");
        Self {
            positions,
            conflicts,
            conflicts_sum,
            fitness: 0.0,
        }
    }

    pub fn get_positions(&self) -> Vec<u16> {
        self.positions.to_vec()
    }

    pub fn get_conflicts(&self) -> Vec<u16> {
        self.conflicts.to_vec()
    }

    pub fn get_conflicts_sum(&self) -> u16 {
        self.conflicts_sum
    }

    pub fn get_fitness(&self) -> f32 {
        self.fitness
    }

    pub fn set_fitness(&mut self, fitness: f32) {
        self.fitness = fitness;
    }
}

pub fn generate_distinct_random_values(size: u16) -> Vec<u16> {
    let mut out_map = HashSet::new();
    while out_map.len() < size as usize {
        out_map.insert(random::<u16>() % size);
    }
    out_map.into_iter().collect::<Vec<_>>()
}

fn count_conflicts(positions: &[u16]) -> Vec<u16> {
    let size = positions.len();
    let mut conflicts = vec![0; size];
    for x_two in 0..size - 1 {
        for x_one in x_two + 1..size {
            let distance = x_one - x_two;
            let y_one = positions[x_one];
            let y_two = positions[x_two];
            if y_one.abs_diff(y_two) == distance as u16 {
                log::trace!("found conflict: ({x_one},{y_one}) -> ({x_two},{y_two})");
                conflicts[x_one] += 1;
                conflicts[x_two] += 1;
            }
        }
    }
    conflicts
}

#[cfg(test)]
mod tests {
    use crate::ga::chromosome::{Chromosome, generate_distinct_random_values};

    #[test]
    fn test_initial_values_generator() {
        let result = generate_distinct_random_values(8);
        assert!(result.contains(&0));
        assert!(result.contains(&1));
        assert!(result.contains(&2));
        assert!(result.contains(&3));
        assert!(result.contains(&4));
        assert!(result.contains(&5));
        assert!(result.contains(&6));
        assert!(result.contains(&7));
    }

    #[test]
    fn test_conflicts_counter() {
        let positions = vec![0, 2, 4, 6, 1, 3, 5, 7];
        let chromosome = Chromosome::new(positions);
        let conflicts_sum = chromosome.get_conflicts_sum();
        assert_eq!(conflicts_sum, 1);
        let positions = vec![2, 4, 1, 7, 5, 0, 6, 3];
        let chromosome = Chromosome::new(positions);
        let conflicts_sum = chromosome.get_conflicts_sum();
        assert_eq!(conflicts_sum, 2);
    }
}
