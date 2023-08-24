use rand::random;
use std::collections::HashSet;

pub struct Chromosome {
    positions: Vec<usize>,
    conflicts: Vec<usize>,
    conflicts_sum: usize,
}

impl Chromosome {
    pub fn new(size: usize) -> Self {
        let positions = generate_distinct_random_values(size);
        let conflicts = count_conflicts(&positions);
        let conflicts_sum = conflicts.iter().sum::<usize>() / 2;
        Self {
            positions,
            conflicts,
            conflicts_sum,
        }
    }

    pub fn get_positions(&self) -> Vec<usize> {
        self.positions.to_vec()
    }

    pub fn get_conflicts(&self) -> Vec<usize> {
        self.conflicts.to_vec()
    }

    pub fn get_conflicts_sum(&self) -> usize {
        self.conflicts_sum
    }
}

fn generate_distinct_random_values(size: usize) -> Vec<usize> {
    let mut out_map = HashSet::new();
    while out_map.len() < size {
        out_map.insert(random::<usize>() % size);
    }
    out_map.into_iter().collect::<Vec<_>>()
}

fn count_conflicts(positions: &Vec<usize>) -> Vec<usize> {
    let size = positions.len();
    let mut conflicts = vec![0; size];
    for x_two in 0..size - 1 {
        for x_one in x_two + 1..size {
            let distance = x_one - x_two;
            let y_one = positions[x_one];
            let y_two = positions[x_two];
            log::debug!("counting conflicts: ({x_one},{y_one}) -> ({x_two},{y_two})");
            if y_one == (y_two + distance) {
                conflicts[x_one] += 1;
                conflicts[x_two] += 1;
            }
            if y_two >= distance && y_one == (y_two - distance) {
                conflicts[x_one] += 1;
                conflicts[x_two] += 1;
            }
        }
    }
    conflicts
}

#[cfg(test)]
mod tests {
    use crate::ga::chromosome::{generate_distinct_random_values, Chromosome};

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
        let chromosome = Chromosome::new(2);
        let conflicts_sum = chromosome.get_conflicts_sum();
        assert_eq!(conflicts_sum, 1);
    }
}
