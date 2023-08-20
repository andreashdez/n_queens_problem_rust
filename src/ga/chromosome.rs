use std::collections::HashSet;

use rand::random;

pub struct Chromosome {
    positions: Vec<usize>,
    conflicts: Vec<usize>,
}

impl Chromosome {
    pub fn new(size: usize) -> Self {
        Self {
            positions: generate_distinct_random_values(size),
            conflicts: vec![0; size],
        }
    }

    pub fn get_positions(&self) -> Vec<usize> {
        self.positions.to_vec()
    }

    pub fn get_conflicts(&self) -> Vec<usize> {
        self.conflicts.to_vec()
    }

    pub fn get_conflicts_sum(&self) -> usize {
        self.conflicts.iter().sum::<usize>() / 2
    }

    pub fn count_conflicts(&mut self) {
        let size = self.positions.len();
        for x_two in 0..size - 1 {
            for x_one in x_two + 1..size {
                let distance = x_one - x_two;
                let y_one = *self.positions.get(x_one).unwrap();
                let y_two = *self.positions.get(x_two).unwrap();
                log::debug!("counting conflicts: ({x_one},{y_one}) -> ({x_two},{y_two})");
                if y_one == (y_two + distance) {
                    *self.conflicts.get_mut(x_one).unwrap() += 1;
                    *self.conflicts.get_mut(x_two).unwrap() += 1;
                }
                if y_two >= distance && y_one == (y_two - distance) {
                    *self.conflicts.get_mut(x_one).unwrap() += 1;
                    *self.conflicts.get_mut(x_two).unwrap() += 1;
                }
            }
        }
    }
}

fn generate_distinct_random_values(size: usize) -> Vec<usize> {
    let mut out_map = HashSet::new();
    while out_map.len() < size {
        out_map.insert(random::<usize>() % size);
    }
    out_map.into_iter().collect::<Vec<_>>()
}

pub fn draw_board(positions: Vec<usize>, conflicts: Vec<usize>) {
    let size = positions.len();
    draw_top_row(size);
    for y in 0..size {
        print!("║ ");
        for x in 0..size {
            let y_position = *positions.get(x).unwrap();
            if y_position == y {
                let current_conflicts = conflicts.get(x).unwrap();
                print!("{current_conflicts:0>2}");
            } else {
                print!("  ");
            }
            if x < size - 1 {
                print!(" │ ")
            } else {
                println!(" ║")
            }
        }
        if y < size - 1 {
            draw_middle_row(size);
        }
    }
    draw_bottom_row(size);
}

fn draw_top_row(size: usize) {
    let mut s = String::from("╔══");
    for _ in 0..(size - 1) {
        s.push_str("══╤══");
    }
    s.push_str("══╗");
    println!("{s}");
}

fn draw_middle_row(size: usize) {
    let mut s = String::from("╟──");
    for _ in 0..(size - 1) {
        s.push_str("──┼──");
    }
    s.push_str("──╢");
    println!("{s}");
}

fn draw_bottom_row(size: usize) {
    let mut s = String::from("╚══");
    for _ in 0..(size - 1) {
        s.push_str("══╧══");
    }
    s.push_str("══╝");
    println!("{s}");
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
        let mut chromosome = Chromosome::new(2);
        chromosome.count_conflicts();
        let conflicts_sum = chromosome.get_conflicts_sum();
        assert_eq!(conflicts_sum, 1);
    }
}
