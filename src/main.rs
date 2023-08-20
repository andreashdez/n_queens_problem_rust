use crate::ga::chromosome::{Chromosome, draw_board};

pub mod ga;

fn main() {
    simple_logger::init().unwrap();
    let mut chromosome = Chromosome::new(8);
    chromosome.count_conflicts();
    let conflicts_sum = chromosome.get_conflicts_sum();
    println!("Conflicts: {conflicts_sum}");
    draw_board(chromosome.get_positions(), chromosome.get_conflicts());
}
